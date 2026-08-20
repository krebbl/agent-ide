use async_trait::async_trait;
use portable_pty::PtySize;
use russh::{client, Channel, ChannelMsg};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::agent_detect;
use crate::pty_protocol::ProcessInfo;

pub type SessionHandle = Arc<Mutex<client::Handle<ClientHandler>>>;

pub type SshSession = client::Handle<ClientHandler>;
use russh::keys::agent::client::AgentClient;
use russh::keys::PrivateKeyWithHashAlg;
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tracing::info;

use std::time::{Duration, Instant};

/// A request to the background probe task to enumerate the processes running
/// in this remote terminal session.
struct ProbeRequest {
    respond: oneshot::Sender<Vec<ProcessInfo>>,
}

pub struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub async fn connect_ssh(
    host: &str,
    port: u16,
    username: &str,
    auth_method: &str,
    key_path: Option<&str>,
    password: Option<&str>,
) -> Result<client::Handle<ClientHandler>, String> {
    info!("remote_ssh: host={} port={} username={} auth_method={}", host, port, username, auth_method);
    let config = Arc::new(client::Config::default());

    let connect_timeout = if auth_method == "agent" {
        Duration::from_secs(120)
    } else {
        Duration::from_secs(15)
    };

    let mut session = tokio::time::timeout(
        connect_timeout,
        client::connect(config, (host, port), ClientHandler),
    )
    .await
    .map_err(|_| "Connection timed out".to_string())?
    .map_err(|e| format!("Failed to connect: {}", e))?;

    let auth = tokio::time::timeout(Duration::from_secs(60), async {
    match auth_method {
        "key" => {
            let kp = key_path.ok_or("Key path is required for key authentication")?;
            let key = russh_keys::load_secret_key(kp, None)
                .map_err(|e| format!("Failed to load private key: {}", e))?;
            let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key), None);
            let auth_result = session
                .authenticate_publickey(username.to_string(), key_with_hash)
                .await
                .map_err(|e| format!("Key authentication failed: {}", e))?;
            if !auth_result.success() {
                return Err("Key authentication rejected".to_string());
            }
        }
        "agent" => {
            let agent_path = one_password_agent_socket()
                .or_else(|| std::env::var("SSH_AUTH_SOCK").ok().filter(|s| !s.is_empty()).map(PathBuf::from))
                .ok_or("No 1Password agent socket found and SSH_AUTH_SOCK is not set")?;

            let stream = UnixStream::connect(&agent_path)
                .await
                .map_err(|e| format!("Failed to connect to SSH agent socket: {}", e))?;
            let mut agent = AgentClient::connect(stream);
            let identities = agent
                .request_identities()
                .await
                .map_err(|e| format!("Failed to get identities from SSH agent: {}", e))?;
            if identities.is_empty() {
                return Err("SSH agent has no keys. If you use 1Password, make sure it is unlocked and the SSH agent is enabled.".to_string());
            }

            let mut authenticated = false;
            let mut last_error: Option<String> = None;
            for key in &identities {
                match session
                    .authenticate_publickey_with(username.to_string(), key.clone(), None, &mut agent)
                    .await
                {
                    Ok(auth) if auth.success() => {
                        authenticated = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(e) => last_error = Some(format!("{}", e)),
                }
            }
            if !authenticated {
                return Err(last_error.unwrap_or_else(|| {
                    "SSH agent authentication rejected. None of the available keys were accepted by the server.".to_string()
                }));
            }
        }
        "password" => {
            let pwd = password.ok_or("Password is required for password authentication")?;
            let auth_result = session
                .authenticate_password(username.to_string(), pwd.to_string())
                .await
                .map_err(|e| format!("Password authentication failed: {}", e))?;
            if !auth_result.success() {
                return Err("Password authentication rejected".to_string());
            }
        }
        _ => return Err(format!("Unsupported auth method: {}", auth_method)),
    }
    Ok(())
    })
    .await
    .map_err(|_| "SSH authentication timed out".to_string())?;
    auth?;

    Ok(session)
}

fn one_password_agent_socket() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let legacy = PathBuf::from(&home)
                .join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock");
            if legacy.exists() {
                return Some(legacy);
            }
            let symlink = PathBuf::from(&home).join(".1password/agent.sock");
            if symlink.exists() {
                return Some(symlink);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let socket = PathBuf::from(&home).join(".1password/agent.sock");
            if socket.exists() {
                return Some(socket);
            }
        }
        if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
            let socket = PathBuf::from(&xdg).join("1password/agent.sock");
            if socket.exists() {
                return Some(socket);
            }
        }
    }

    None
}

pub struct RemotePtyEngine {
    input_tx: mpsc::Sender<String>,
    resize_tx: mpsc::Sender<PtySize>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    probe_tx: mpsc::UnboundedSender<ProbeRequest>,
    probe_shutdown_tx: Option<oneshot::Sender<()>>,
}

impl RemotePtyEngine {
    pub async fn spawn(
        session_id: String,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        ssh_session: SessionHandle,
        event_tx: tokio::sync::mpsc::Sender<(String, crate::pty_engine::EngineEvent)>,
        attach: bool,
    ) -> Result<Self, String> {
        let channel = {
            let handle = ssh_session.lock().await;
            tokio::time::timeout(Duration::from_secs(30), handle.channel_open_session())
                .await
                .map_err(|_| "Timed out opening SSH channel".to_string())?
                .map_err(|e| format!("Failed to open SSH channel: {}", e))?
        };

        tokio::time::timeout(
            Duration::from_secs(15),
            channel.request_pty(false, "xterm-256color", cols as u32, rows as u32, 0, 0, &[]),
        )
        .await
        .map_err(|_| "request_pty timed out".to_string())?
        .map_err(|e| format!("request_pty failed: {}", e))?;
        tokio::time::timeout(Duration::from_secs(15), channel.request_shell(true))
            .await
            .map_err(|_| "request_shell timed out".to_string())?
            .map_err(|e| format!("request_shell failed: {}", e))?;

        if attach {
            let tmux_cmd = format!(
                "exec tmux set -g status off \\; new-session -A -s {} 2>/dev/null || exec ${{SHELL:-/bin/sh}} -l\n",
                shell_escape(&session_id)
            );
            let _ = channel.data(std::io::Cursor::new(tmux_cmd.into_bytes())).await;
        } else {
            if let Some(ref dir) = cwd {
                let cmd = format!("cd {}\n", shell_escape(dir));
                let _ = channel.data(std::io::Cursor::new(cmd.into_bytes())).await;
            }
            // Invisible identity marker: ask the fresh remote shell for its
            // controlling terminal. `ps` reports that tty for every process
            // attached to this terminal (shell, foreground and background
            // jobs). OSC 1338 is ignored by real terminals.
            let marker = b"printf '\\033]1338;AI_TTY=%s\\033\\\\' \"$(tty)\"\n";
            let _ = channel
                .data(std::io::Cursor::new(marker.to_vec()))
                .await;
        }

        let (input_tx, input_rx) = mpsc::channel::<String>(64);
        let (resize_tx, resize_rx) = mpsc::channel::<PtySize>(16);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // Controlling terminal of the remote session, learned from the marker
        // the shell echoes right after spawn.
        let session_key: Arc<std::sync::Mutex<Option<String>>> =
            Arc::new(std::sync::Mutex::new(None));

        // Background probe task: runs `ps` on the server over a second SSH
        // channel on demand.
        let (probe_tx, probe_rx) = mpsc::unbounded_channel::<ProbeRequest>();
        let (probe_shutdown_tx, probe_shutdown_rx) = oneshot::channel::<()>();
        {
            let probe_session = ssh_session.clone();
            let probe_key = Arc::clone(&session_key);
            tokio::spawn(async move {
                run_probe_loop(probe_session, probe_key, probe_rx, probe_shutdown_rx).await;
            });
        }

        let engine = RemotePtyEngine {
            input_tx,
            resize_tx,
            shutdown_tx: Some(shutdown_tx),
            probe_tx,
            probe_shutdown_tx: Some(probe_shutdown_tx),
        };

        let session_handle = ssh_session.clone();
        tokio::spawn(async move {
            run_remote_terminal(
                session_id,
                channel,
                session_handle,
                session_key,
                event_tx,
                input_rx,
                resize_rx,
                shutdown_rx,
            )
            .await;
        });

        Ok(engine)
    }
}

#[async_trait]
impl crate::pty_engine::PtyEngine for RemotePtyEngine {
    fn write(&self, data: &[u8]) -> Result<(), String> {
        let tx = self.input_tx.clone();
        let text = String::from_utf8_lossy(data).to_string();
        tokio::spawn(async move {
            let _ = tx.send(text).await;
        });
        Ok(())
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let tx = self.resize_tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).await;
        });
        Ok(())
    }

    fn kill(&self) -> Result<(), String> {
        Ok(())
    }

    fn process_group_id(&self) -> Option<i32> {
        None
    }

    async fn probe_processes(&self) -> Vec<ProcessInfo> {
        let (tx, rx) = oneshot::channel();
        if self.probe_tx.send(ProbeRequest { respond: tx }).is_err() {
            return Vec::new();
        }
        tokio::time::timeout(Duration::from_secs(8), rx)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default()
    }
}

impl Drop for RemotePtyEngine {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(tx) = self.probe_shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

async fn run_remote_terminal(
    session_id: String,
    mut channel: Channel<client::Msg>,
    session_handle: SessionHandle,
    session_key: Arc<std::sync::Mutex<Option<String>>>,
    event_tx: tokio::sync::mpsc::Sender<(String, crate::pty_engine::EngineEvent)>,
    mut input_rx: mpsc::Receiver<String>,
    mut resize_rx: mpsc::Receiver<PtySize>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let mut osc_state = Vec::new();
    let mut title_state = Vec::new();
    let mut tty_marker_state = Vec::new();
    let mut exit_code: Option<i32> = None;
    let mut last_agent_probe: Option<Instant> = None;
    let mut probe_in_flight = false;
    let mut detected_agent: Option<String> = None;
    let (probe_result_tx, mut probe_result_rx) =
        mpsc::unbounded_channel::<Vec<ProcessInfo>>();

    // Probe the remote process table when this session produces output
    // (throttled to once per second, one probe in flight). Many remote
    // shells never emit OSC-133 markers, so output activity — not the Busy
    // event — is the trigger. Idle sessions produce no output and thus no
    // probes: zero SSH overhead when nothing is running.
    let mut schedule_probe = |last: &mut Option<Instant>,
                              in_flight: &mut bool,
                              key: Option<String>| {
        let due = last
            .map(|t| t.elapsed() >= Duration::from_secs(1))
            .unwrap_or(true);
        if !*in_flight && due {
            if let Some(key) = key {
                *last = Some(Instant::now());
                *in_flight = true;
                let h = session_handle.clone();
                let tx = probe_result_tx.clone();
                tokio::spawn(async move {
                    let procs = remote_ps(&h, Some(key.as_str())).await;
                    let _ = tx.send(procs);
                });
            }
        }
    };

    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        if let Some(tty) = agent_detect::scan_ai_tty_marker(&mut tty_marker_state, data.as_ref()) {
                            *session_key.lock().unwrap() = Some(tty);
                        }
                        match crate::pty::scan_osc133_command(&mut osc_state, data.as_ref()) {
                            Some(crate::pty::Osc133Event::End) => {
                                let _ = event_tx.send((session_id.clone(), crate::pty_engine::EngineEvent::Idle)).await;
                            }
                            Some(crate::pty::Osc133Event::Start) => {
                                let _ = event_tx.send((session_id.clone(), crate::pty_engine::EngineEvent::Busy)).await;
                            }
                            None => {}
                        }
                        if let Some(title) = crate::pty::scan_osc_title(&mut title_state, data.as_ref()) {
                            let _ = event_tx.send((session_id.clone(), crate::pty_engine::EngineEvent::Title(title))).await;
                        }
                        let encoded = STANDARD.encode(data.as_ref());
                        let _ = event_tx.send((session_id.clone(), crate::pty_engine::EngineEvent::Output(encoded))).await;
                    }
                    Some(ChannelMsg::ExtendedData { data, ext }) if ext == 1 => {
                        let encoded = STANDARD.encode(data.as_ref());
                        let _ = event_tx.send((session_id.clone(), crate::pty_engine::EngineEvent::Output(encoded))).await;
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        exit_code = Some(exit_status as i32);
                        break;
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                    _ => {}
                }
                schedule_probe(
                    &mut last_agent_probe,
                    &mut probe_in_flight,
                    session_key.lock().unwrap().clone(),
                );
            }
            Some(data) = input_rx.recv() => {
                let _ = channel.data(std::io::Cursor::new(data.into_bytes())).await;
            }
            Some(size) = resize_rx.recv() => {
                let _ = channel.window_change(
                    size.cols as u32,
                    size.rows as u32,
                    size.pixel_width as u32,
                    size.pixel_height as u32,
                ).await;
            }
            _ = &mut shutdown_rx => {
                let _ = channel.eof().await;
                let _ = channel.close().await;
                break;
            }
            Some(procs) = probe_result_rx.recv() => {
                probe_in_flight = false;
                match agent_detect::detect_agent_in_processes(&procs) {
                    Some(name) => {
                        if detected_agent.as_deref() != Some(name.as_str()) {
                            detected_agent = Some(name.clone());
                            let _ = event_tx.send((
                                session_id.clone(),
                                crate::pty_engine::EngineEvent::Agent(Some(name)),
                            )).await;
                        }
                    }
                    None => {
                        if detected_agent.take().is_some() {
                            let _ = event_tx.send((
                                session_id.clone(),
                                crate::pty_engine::EngineEvent::Agent(None),
                            )).await;
                        }
                    }
                }
            }
        }
    }

    let _ = event_tx.send((session_id, crate::pty_engine::EngineEvent::Exit(exit_code))).await;
}

/// Run `ps -A -o tty=,pid=,comm=,args=` on the remote server over a fresh
/// channel and keep the rows whose controlling terminal matches the session's
/// captured tty — every process in this terminal.
async fn remote_ps(session: &SessionHandle, tty: Option<&str>) -> Vec<ProcessInfo> {
    let Some(tty) = tty else {
        return Vec::new();
    };
    let channel = {
        let handle = session.lock().await;
        match tokio::time::timeout(
            Duration::from_secs(10),
            handle.channel_open_session(),
        )
        .await
        {
            Ok(Ok(ch)) => Some(ch),
            _ => None,
        }
    };
    let Some(channel) = channel else {
        return Vec::new();
    };
    if channel
        .exec(true, "ps -A -o tty=,pid=,comm=,args=")
        .await
        .is_err()
    {
        let _ = channel.close().await;
        return Vec::new();
    }
    let mut channel = channel;
    let mut out = Vec::new();
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => out.extend_from_slice(data.as_ref()),
            Some(ChannelMsg::ExtendedData { data, .. }) => out.extend_from_slice(data.as_ref()),
            Some(ChannelMsg::ExitStatus { .. })
            | Some(ChannelMsg::Eof)
            | Some(ChannelMsg::Close)
            | None => break,
            _ => {}
        }
    }
    let _ = channel.close().await;
    agent_detect::parse_processes_by_tty(&String::from_utf8_lossy(&out), tty)
}

/// Serve on-demand process-list requests over a second SSH channel.
async fn run_probe_loop(
    session: SessionHandle,
    session_key: Arc<std::sync::Mutex<Option<String>>>,
    mut rx: mpsc::UnboundedReceiver<ProbeRequest>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            Some(req) = rx.recv() => {
                let key = session_key.lock().unwrap().clone();
                let procs = remote_ps(&session, key.as_deref()).await;
                let _ = req.respond.send(procs);
            }
            _ = &mut shutdown_rx => break,
        }
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}
