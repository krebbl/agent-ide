use portable_pty::PtySize;
use russh::{client, Channel, ChannelMsg};
use std::sync::Arc;
use tokio::sync::Mutex;

pub type SessionHandle = Arc<Mutex<client::Handle<ClientHandler>>>;

pub type SshSession = client::Handle<ClientHandler>;
use russh::keys::agent::client::AgentClient;
use russh::keys::PrivateKeyWithHashAlg;
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use std::time::Duration;

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
        let mut channel = ssh_session
            .lock()
            .await
            .channel_open_session()
            .await
            .map_err(|e| format!("Failed to open SSH channel: {}", e))?;

        channel
            .request_pty(false, "xterm-256color", cols as u32, rows as u32, 0, 0, &[])
            .await
            .map_err(|e| format!("request_pty failed: {}", e))?;
        channel
            .request_shell(true)
            .await
            .map_err(|e| format!("request_shell failed: {}", e))?;

        if attach {
            let tmux_cmd = format!(
                "exec tmux set -g status off \\; new-session -A -s {} 2>/dev/null || exec ${{SHELL:-/bin/sh}} -l\n",
                shell_escape(&session_id)
            );
            let _ = channel.data(std::io::Cursor::new(tmux_cmd.into_bytes())).await;
        } else if let Some(ref dir) = cwd {
            let cmd = format!("cd {}\n", shell_escape(dir));
            let _ = channel.data(std::io::Cursor::new(cmd.into_bytes())).await;
        }

        let (input_tx, mut input_rx) = mpsc::channel::<String>(64);
        let (resize_tx, mut resize_rx) = mpsc::channel::<PtySize>(16);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        let engine = RemotePtyEngine {
            input_tx,
            resize_tx,
            shutdown_tx: Some(shutdown_tx),
        };

        let session_handle = ssh_session.clone();
        tokio::spawn(async move {
            run_remote_terminal(session_id, channel, session_handle, event_tx, input_rx, resize_rx, shutdown_rx).await;
        });

        Ok(engine)
    }
}

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
}

impl Drop for RemotePtyEngine {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

async fn run_remote_terminal(
    session_id: String,
    mut channel: Channel<client::Msg>,
    _session_handle: SessionHandle,
    event_tx: tokio::sync::mpsc::Sender<(String, crate::pty_engine::EngineEvent)>,
    mut input_rx: mpsc::Receiver<String>,
    mut resize_rx: mpsc::Receiver<PtySize>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let mut osc_state = Vec::new();
    let mut exit_code: Option<i32> = None;

    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        match crate::pty::scan_osc133_command(&mut osc_state, data.as_ref()) {
                            Some(crate::pty::Osc133Event::End) => {
                                let _ = event_tx.send((session_id.clone(), crate::pty_engine::EngineEvent::Idle)).await;
                            }
                            Some(crate::pty::Osc133Event::Start) => {
                                let _ = event_tx.send((session_id.clone(), crate::pty_engine::EngineEvent::Busy)).await;
                            }
                            None => {}
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
        }
    }

    let _ = event_tx.send((session_id, crate::pty_engine::EngineEvent::Exit(exit_code))).await;
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}
