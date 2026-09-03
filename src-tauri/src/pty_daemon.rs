use serde_json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, Mutex as TokioMutex};
use tracing::{error, info, warn};

use crate::pty_engine::{EngineEvent, LocalPtyEngine, PtyEngine};
use crate::pty_protocol::{DaemonEvent, DaemonRequest, SessionMeta};
use crate::remote_ssh::{self, RemotePtyEngine, SessionHandle};
use crate::agent_detect;
use crate::agents;

struct DaemonSession {
    meta: SessionMeta,
    engine: Option<Arc<dyn PtyEngine>>,
    /// Set when `is_busy` was raised by a spinner title rather than the
    /// foreground/OSC-133 signals; a later non-spinner title change may
    /// then clear it.
    title_busy: bool,
}

/// Augment a persisted agent-session command so a reboot restores the
/// conversation: an injected `--session-id <id>` pins the exact conversation
/// and maps to `--resume <id>`; otherwise the agent's continue flag is
/// appended. `live_conversation` (from the SessionStart marker hook) is the
/// conversation the user actually switched to inside the agent and, when
/// present, wins over the pinned id. Non-agent argv and agents without
/// resume support pass through unchanged.
fn restore_argv(argv: &[String], live_conversation: Option<&str>) -> Vec<String> {
    if let Some(pos) = argv.iter().position(|a| a == "--session-id") {
        if let Some(id) = argv.get(pos + 1) {
            let mut out: Vec<String> = argv
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != pos && *i != pos + 1)
                .map(|(_, a)| a.clone())
                .collect();
            out.push("--resume".to_string());
            out.push(live_conversation.unwrap_or(id).to_string());
            return out;
        }
    }
    let Some(binary) = argv.first() else {
        return argv.to_vec();
    };
    if agents::pins_conversation(argv) {
        return argv.to_vec();
    }
    let agent =
        agent_detect::matches_agent(binary, binary, agent_detect::KNOWN_AGENT_BINARIES);
    match agent.as_deref().and_then(agents::exact_resume_flag) {
        // The marker hook knows the live conversation: resume exactly it.
        Some(flag) if !argv.iter().any(|arg| arg == flag) => {
            if let Some(live) = live_conversation {
                let mut out = argv.to_vec();
                out.push(flag.to_string());
                out.push(live.to_string());
                return out;
            }
            // No id known: fuzzy continue-latest where supported.
            match agent.as_deref().and_then(agents::resume_flag) {
                Some(fallback) if !argv.iter().any(|arg| arg == fallback) => {
                    let mut out = argv.to_vec();
                    out.push(fallback.to_string());
                    out
                }
                _ => argv.to_vec(),
            }
        }
        _ => argv.to_vec(),
    }
}

/// A parsed conversation marker file.
#[derive(Debug, Clone)]
struct Marker {
    id: String,
    agent: Option<String>,
    pid: Option<u32>,
}

/// Parse a marker file payload: `{ session_id, agent?, pid? }`.
fn parse_marker(data: &str) -> Option<(String, Option<String>, Option<u32>)> {
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    let id = value.get("session_id")?.as_str()?.to_string();
    let agent = value
        .get("agent")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let pid = value.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);
    Some((id, agent, pid))
}

/// True when the marker's conversation is still resumable. Markers without
/// an agent (claude/pi/omp) are persistent. opencode conversations are not
/// pinned — its marker carries the agent's pid and dies with the process.
fn marker_live(agent: &Option<String>, pid: Option<u32>) -> bool {
    if agent.as_deref() != Some("opencode") {
        return true;
    }
    match pid {
        None => true,
        Some(pid) => unsafe { libc::kill(pid as i32, 0) == 0 },
    }
}

/// The conversation id pinned by an injected `--session-id <id>`, if present.
fn pinned_conversation_id(argv: &[String]) -> Option<String> {
    let pos = argv.iter().position(|a| a == "--session-id")?;
    argv.get(pos + 1).cloned()
}

struct SshProject {
    host: String,
    port: u16,
    username: String,
    auth_method: String,
    key_path: Option<String>,
    password: Option<String>,
    session: Option<SessionHandle>,
}

struct SshManager {
    projects: Mutex<HashMap<String, SshProject>>,
}

impl SshManager {
    fn new() -> Self {
        Self {
            projects: Mutex::new(HashMap::new()),
        }
    }

    fn register(
        &self,
        project_id: String,
        host: String,
        port: u16,
        username: String,
        auth_method: String,
        key_path: Option<String>,
        password: Option<String>,
    ) {
        let mut projects = self.projects.lock().unwrap();
        projects.insert(
            project_id,
            SshProject {
                host,
                port,
                username,
                auth_method,
                key_path,
                password,
                session: None,
            },
        );
    }

    async fn ensure_connection(&self, project_id: &str) -> Result<SessionHandle, String> {
        let project = {
            let projects = self.projects.lock().unwrap();
            projects
                .get(project_id)
                .ok_or("SSH project not registered")?
                .clone()
        };

        let session = remote_ssh::connect_ssh(
            &project.host,
            project.port,
            &project.username,
            &project.auth_method,
            project.key_path.as_deref(),
            project.password.as_deref(),
        )
        .await?;

        let session = Arc::new(TokioMutex::new(session));
        let mut projects = self.projects.lock().unwrap();
        if let Some(p) = projects.get_mut(project_id) {
            p.session = Some(session.clone());
        }
        Ok(session)
    }
}

impl Clone for SshProject {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            auth_method: self.auth_method.clone(),
            key_path: self.key_path.clone(),
            password: self.password.clone(),
            session: self.session.as_ref().map(Arc::clone),
        }
    }
}

pub struct PtyDaemon {
    socket_path: PathBuf,
    sessions: Arc<Mutex<HashMap<String, DaemonSession>>>,
    persistence_path: PathBuf,
    /// PATH-shim directory that pins agent conversations to terminal
    /// session ids; `None`-equivalent when unwritable.
    shim_dir: Option<PathBuf>,
    /// Directory of SessionStart marker files (`<pty-id>.conversation`)
    /// written by the agent hook; watched for live conversation switches.
    marker_dir: PathBuf,
    client_tx: Arc<Mutex<Option<mpsc::UnboundedSender<DaemonEvent>>>>,
    event_tx: mpsc::Sender<(String, EngineEvent)>,
    _event_rx_handle: Option<tokio::task::JoinHandle<()>>,
    ssh_manager: Arc<SshManager>,
}

impl PtyDaemon {
    pub fn new(socket_path: PathBuf, persistence_path: PathBuf) -> Self {
        let (event_tx, event_rx) = mpsc::channel::<(String, EngineEvent)>(256);
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let client_tx = Arc::new(Mutex::new(None::<mpsc::UnboundedSender<DaemonEvent>>));

        let event_rx_handle = {
            let sessions = Arc::clone(&sessions);
            let client_tx = Arc::clone(&client_tx);
            let persistence_path = persistence_path.clone();
            tokio::spawn(async move {
                Self::event_broadcaster(event_rx, sessions, client_tx, persistence_path).await;
            })
        };

        let shim_dir = persistence_path
            .parent()
            .map(|p| p.join("agent-shims"))
            .unwrap_or_else(|| persistence_path.with_extension("shims"));
        let marker_dir = persistence_path
            .parent()
            .map(|p| p.join("agent-session-markers"))
            .unwrap_or_else(|| persistence_path.with_extension("markers"));
        let _ = agents::ensure_session_id_shims(&shim_dir, &marker_dir);

        Self {
            socket_path,
            marker_dir,
            sessions,
            persistence_path,
            shim_dir: Some(shim_dir),
            client_tx,
            event_tx,
            _event_rx_handle: Some(event_rx_handle),
            ssh_manager: Arc::new(SshManager::new()),
        }
    }
    pub async fn run(self) -> Result<(), String> {
        let _ = std::fs::remove_file(&self.socket_path);
        let listener = UnixListener::bind(&self.socket_path)
            .map_err(|e| format!("Failed to bind daemon socket: {}", e))?;
        info!(socket = %self.socket_path.display(), "pty daemon listening");

        self.load_sessions();
        let daemon = Arc::new(self);

        // Watch the SessionStart marker directory: when an agent starts or
        // switches a conversation in-app (`/resume`), push the new live
        // conversation id to the frontend and persist it.
        {
            let daemon = Arc::clone(&daemon);
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
                loop {
                    tick.tick().await;
                    for (session_id, conversation_id) in daemon.refresh_conversation_markers() {
                        info!(session_id = %session_id, conversation_id = ?conversation_id, "agent conversation changed");
                        PtyDaemon::send_to_client(
                            &daemon.client_tx,
                            DaemonEvent::Conversation {
                                session_id,
                                conversation_id,
                            },
                        );
                    }
                    PtyDaemon::persist(&daemon.sessions, &daemon.persistence_path);
                }
            });
        }

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    info!("pty daemon client connected");
                    let daemon = Arc::clone(&daemon);
                    let sessions = Arc::clone(&daemon.sessions);
                    let persistence_path = daemon.persistence_path.clone();
                    let event_tx = daemon.event_tx.clone();
                    let client_tx_cell = Arc::clone(&daemon.client_tx);

                    let (client_tx, mut client_rx) = mpsc::unbounded_channel::<DaemonEvent>();
                    *client_tx_cell.lock().unwrap() = Some(client_tx);
                    let ssh_manager = Arc::clone(&daemon.ssh_manager);

                    let (read_half, mut write_half) = stream.into_split();

                    tokio::spawn(async move {
                        while let Some(event) = client_rx.recv().await {
                            let json = serde_json::to_string(&event).unwrap_or_default();
                            if write_half
                                .write_all(format!("{}\n", json).as_bytes())
                                .await
                                .is_err()
                            {
                                break;
                            }
                            let _ = write_half.flush().await;
                        }
                    });

                    tokio::spawn(async move {
                        let mut reader = BufReader::new(read_half);
                        let mut line = String::new();
                        loop {
                            line.clear();
                            match reader.read_line(&mut line).await {
                                Ok(0) => break,
                                Ok(_) => {
                                    if let Ok(req) =
                                        serde_json::from_str::<DaemonRequest>(line.trim())
                                    {
                                        // Clone the optional sender out of the
                                        // lock guard: the request handler may
                                        // await (remote probes), and holding a
                                        // guard across await is not Send.
                                        let client_tx_opt =
                                            client_tx_cell.lock().unwrap().clone();
                                        daemon.handle_request(
                                            req,
                                            &sessions,
                                            &persistence_path,
                                            &event_tx,
                                            client_tx_opt.as_ref(),
                                            &ssh_manager,
                                        ).await;
                                    }
                                }
                                Err(e) => {
                                    warn!("daemon socket read error: {}", e);
                                    break;
                                }
                            }
                        }
                        info!("pty daemon client read loop ended");
                    });
                }
                Err(e) => {
                    error!("daemon accept error: {}", e);
                }
            }
        }
    }

    async fn event_broadcaster(
        mut event_rx: mpsc::Receiver<(String, EngineEvent)>,
        sessions: Arc<Mutex<HashMap<String, DaemonSession>>>,
        client_tx: Arc<Mutex<Option<mpsc::UnboundedSender<DaemonEvent>>>>,
        persistence_path: PathBuf,
    ) {
        while let Some((session_id, ev)) = event_rx.recv().await {
            let mut map = sessions.lock().unwrap();
            let mut dirty = false;
            let event = match ev {
                EngineEvent::Output(data) => {
                    drop(map);
                    let _ = Self::send_to_client(
                        &client_tx,
                        DaemonEvent::Output {
                            session_id,
                            data,
                        },
                    );
                    continue;
                }
                EngineEvent::Title(title) => {
                    // Deduplicate: shells rewrite an identical title on every
                    // prompt. A changed title is also activity evidence:
                    // coding agents animate a spinner glyph in the title while
                    // working, which is the one busy signal that reaches us
                    // from remote shells and tmux panes that emit no OSC-133
                    // markers. Clearing stays owned by the regular signals
                    // unless this session's busy was raised by a title.
                    let event = match map.get_mut(&session_id) {
                        Some(session) if session.meta.title != title => {
                            session.meta.title = title.clone();
                            dirty = true;
                            if title_has_spinner(&title) {
                                if !session.meta.is_busy {
                                    session.meta.is_busy = true;
                                    session.title_busy = true;
                                    Some(DaemonEvent::Busy {
                                        session_id: session_id.clone(),
                                        title,
                                    })
                                } else {
                                    Some(DaemonEvent::Title {
                                        session_id: session_id.clone(),
                                        title,
                                    })
                                }
                            } else if session.title_busy {
                                session.meta.is_busy = false;
                                session.title_busy = false;
                                Some(DaemonEvent::Idle {
                                    session_id: session_id.clone(),
                                    title,
                                })
                            } else {
                                Some(DaemonEvent::Title {
                                    session_id: session_id.clone(),
                                    title,
                                })
                            }
                        }
                        _ => None,
                    };
                    drop(map);
                    if dirty {
                        Self::persist(&sessions, &persistence_path);
                    }
                    if let Some(event) = event {
                        let _ = Self::send_to_client(&client_tx, event);
                    }
                    continue;
                }
                EngineEvent::Agent(sighting) => {
                    // `sighting` is the live foreground agent (None once it
                    // finishes). Keep `agent_name` sticky (session history)
                    // and `agent_active` live so the UI can pick the icon.
                    // The first agent seen in a shell session also records
                    // its command line as the session's argv, so a reboot
                    // restores the agent (with resume) instead of a bare
                    // shell. Explicitly spawned sessions keep their argv.
                    if let Some(session) = map.get_mut(&session_id) {
                        let mut changed = false;
                        if session.meta.agent_active != sighting.is_some() {
                            session.meta.agent_active = sighting.is_some();
                            changed = true;
                        }
                        if let Some(s) = sighting.as_ref() {
                            if session.meta.agent_name.as_deref() != Some(s.name.as_str()) {
                                session.meta.agent_name = Some(s.name.clone());
                                changed = true;
                            }
                            if session.meta.argv.is_none() {
                                session.meta.argv = Some(
                                    s.command.split_whitespace().map(str::to_string).collect(),
                                );
                                changed = true;
                            }
                            // Before any in-app switch, the live conversation
                            // is the pinned id (`--session-id`). The marker
                            // hook only reports resume/clear/compact, so seed
                            // the initial conversation from the pin.
                            if session.meta.conversation_id.is_none() {
                                session.meta.conversation_id = session
                                    .meta
                                    .argv
                                    .as_ref()
                                    .and_then(|argv| pinned_conversation_id(argv));
                                changed |= session.meta.conversation_id.is_some();
                            }
                        }
                        if changed {
                            dirty = true;
                        }
                    }
                    let event = DaemonEvent::Agent {
                        session_id,
                        name: sighting.map(|s| s.name),
                    };
                    drop(map);
                    if dirty {
                        Self::persist(&sessions, &persistence_path);
                    }
                    let _ = Self::send_to_client(&client_tx, event);
                    continue;
                }
                EngineEvent::Idle => {
                    if let Some(session) = map.get_mut(&session_id) {
                        if session.meta.is_busy {
                            session.meta.is_busy = false;
                            dirty = true;
                        }
                        session.title_busy = false;
                    }
                    let title = map
                        .get(&session_id)
                        .map(|s| s.meta.title.clone())
                        .unwrap_or_else(|| "Terminal".to_string());
                    DaemonEvent::Idle { session_id, title }
                }
                EngineEvent::Busy => {
                    if let Some(session) = map.get_mut(&session_id) {
                        if !session.meta.is_busy {
                            session.meta.is_busy = true;
                            dirty = true;
                        }
                    }
                    let title = map
                        .get(&session_id)
                        .map(|s| s.meta.title.clone())
                        .unwrap_or_else(|| "Terminal".to_string());
                    DaemonEvent::Busy { session_id, title }
                }
                EngineEvent::Exit(exit_code) => {
                    map.remove(&session_id);
                    dirty = true;
                    DaemonEvent::Exit { session_id, exit_code }
                }
            };
            drop(map);
            if dirty {
                Self::persist(&sessions, &persistence_path);
            }
            let _ = Self::send_to_client(&client_tx, event);
        }
    }

    fn send_to_client(
        client_tx: &Arc<Mutex<Option<mpsc::UnboundedSender<DaemonEvent>>>>,
        event: DaemonEvent,
    ) -> Result<(), String> {
        let guard = client_tx.lock().unwrap();
        if let Some(tx) = guard.as_ref() {
            tx.send(event).map_err(|_| "client disconnected".to_string())
        } else {
            Ok(())
        }
    }

    async fn handle_request(
        &self,
        req: DaemonRequest,
        sessions: &Arc<Mutex<HashMap<String, DaemonSession>>>,
        persistence_path: &PathBuf,
        event_tx: &mpsc::Sender<(String, EngineEvent)>,
        client_tx: Option<&mpsc::UnboundedSender<DaemonEvent>>,
        ssh_manager: &Arc<SshManager>,
    ) {
        match req {
            DaemonRequest::CreateLocal {
                session_id,
                cwd,
                cols,
                rows,
                project_id,
                worktree_id,
                argv,
            } => {
                if sessions.lock().unwrap().contains_key(&session_id) {
                    warn!(session_id, "session already exists");
                    return;
                }

                let title = basename(cwd.as_deref().unwrap_or("~"));
                let mut meta = SessionMeta {
                    session_id: session_id.clone(),
                    session_type: "local".to_string(),
                    cwd,
                    title: title.clone(),
                    is_busy: false,
                    project_id,
                    worktree_id,
                    agent_name: None,
                    agent_active: false,
                    conversation_id: None,
                    created_at: Self::now_ms(),
                    pgid: None,
                    cols,
                    rows,
                    argv: argv.map(|a| {
                        let hooks = self.shim_dir.as_ref().map(|d| d.join("claude-hooks.json"));
                        agents::with_forced_session_id(&a, &session_id, hooks.as_deref())
                    }),
                };

                let engine = match LocalPtyEngine::spawn(
                    session_id.clone(),
                    meta.cwd.clone(),
                    cols,
                    rows,
                    event_tx.clone(),
                    meta.argv.clone(),
                    self.shim_dir.clone(),
                ) {
                    Ok(e) => e,
                    Err(e) => {
                        if let Some(tx) = client_tx {
                            let _ = tx.send(DaemonEvent::Error {
                                session_id: Some(session_id.clone()),
                                message: e,
                            });
                        }
                        return;
                    }
                };
                meta.pgid = engine.process_group_id();

                let mut map = sessions.lock().unwrap();
                map.insert(
                    session_id.clone(),
                    DaemonSession {
                        meta,
                        engine: Some(Arc::new(engine)),
                        title_busy: false,
                    },
                );
                drop(map);
                Self::persist(sessions, persistence_path);

                if let Some(tx) = client_tx {
                    let _ = tx.send(DaemonEvent::StateSnapshot {
                        session_id,
                        is_busy: false,
                        title,
                    });
                }
            }
            DaemonRequest::CreateRemote {
                session_id,
                project_id,
                cwd,
                cols,
                rows,
                worktree_id,
                attach,
                argv,
            } => {
                if sessions.lock().unwrap().contains_key(&session_id) {
                    warn!(session_id, "session already exists");
                    return;
                }

                let title = basename(cwd.as_deref().unwrap_or("~"));
                let meta = SessionMeta {
                    session_id: session_id.clone(),
                    session_type: "ssh".to_string(),
                    worktree_id,
                    project_id: Some(project_id.clone()),
                    cwd,
                    title: title.clone(),
                    is_busy: false,
                    agent_name: None,
                    conversation_id: None,
                    agent_active: false,
                    created_at: Self::now_ms(),
                    pgid: None,
                    cols,
                    rows,
                    argv,
                };

                // Insert the session immediately (without an engine) so Resize,
                // Write, and Kill requests arriving while the SSH channel is being
                // established are not silently dropped. A Resize lands in
                // meta.cols/rows and is applied to the engine once spawn completes.
                {
                    let mut map = sessions.lock().unwrap();
                    map.insert(
                        session_id.clone(),
                        DaemonSession {
                            meta: meta.clone(),
                            engine: None,
                            title_busy: false,
                        },
                    );
                    drop(map);
                    Self::persist(sessions, persistence_path);
                }

                let ssh_manager = Arc::clone(ssh_manager);
                let event_tx = event_tx.clone();
                let sessions = Arc::clone(sessions);
                let persistence_path = persistence_path.clone();
                let client_tx = client_tx.cloned();

                tokio::spawn(async move {
                    let ssh_session = match ssh_manager.ensure_connection(&project_id).await {
                        Ok(s) => s,
                        Err(e) => {
                            let mut map = sessions.lock().unwrap();
                            map.remove(&session_id);
                            drop(map);
                            PtyDaemon::persist(&sessions, &persistence_path);
                            if let Some(tx) = client_tx {
                                let _ = tx.send(DaemonEvent::Error {
                                    session_id: Some(session_id.clone()),
                                    message: e,
                                });
                            }
                            return;
                        }
                    };

                    let engine = match RemotePtyEngine::spawn(
                        session_id.clone(),
                        meta.cwd.clone(),
                        cols,
                        rows,
                        ssh_session,
                        event_tx.clone(),
                        attach,
                        meta.argv.clone(),
                    )
                    .await
                    {
                        Ok(e) => e,
                        Err(e) => {
                            let mut map = sessions.lock().unwrap();
                            map.remove(&session_id);
                            drop(map);
                            PtyDaemon::persist(&sessions, &persistence_path);
                            if let Some(tx) = client_tx {
                                let _ = tx.send(DaemonEvent::Error {
                                    session_id: Some(session_id.clone()),
                                    message: e,
                                });
                            }
                            return;
                        }
                    };

                    let mut map = sessions.lock().unwrap();
                    if let Some(session) = map.get_mut(&session_id) {
                        if session.engine.is_none() {
                            let (latest_cols, latest_rows) =
                                (session.meta.cols, session.meta.rows);
                            if (latest_cols, latest_rows) != (cols, rows) {
                                let _ = engine.resize(latest_cols, latest_rows);
                            }
                            session.engine = Some(Arc::new(engine));
                        }
                        // If an engine is already attached (concurrent respawn),
                        // drop the redundant one; dropping it closes the channel.
                    }
                    // If the session is gone it was killed while connecting;
                    // dropping the engine closes the SSH channel.
                    drop(map);
                    PtyDaemon::persist(&sessions, &persistence_path);

                    if let Some(tx) = client_tx {
                        let _ = tx.send(DaemonEvent::StateSnapshot {
                            session_id,
                            is_busy: false,
                            title: meta.title,
                        });
                    }
                });
            }
            DaemonRequest::RegisterSshProject {
                project_id,
                host,
                port,
                username,
                auth_method,
                key_path,
                password,
            } => {
                let pid = project_id.clone();
                ssh_manager.register(project_id, host, port, username, auth_method, key_path, password);
                self.respawn_remote_sessions(&pid);
            }
            DaemonRequest::Write { session_id, data } => {
                use base64::{engine::general_purpose::STANDARD, Engine as _};
                let mut map = sessions.lock().unwrap();
                if let Some(session) = map.get_mut(&session_id) {
                    if let Some(engine) = session.engine.as_ref() {
                        if let Ok(bytes) = STANDARD.decode(&data) {
                            let _ = engine.write(&bytes);
                        } else {
                            warn!(session_id = %session_id, "failed to decode write data");
                        }
                    }
                }
            }
            DaemonRequest::Resize { session_id, cols, rows } => {
                let mut map = sessions.lock().unwrap();
                if let Some(session) = map.get_mut(&session_id) {
                    if let Some(engine) = session.engine.as_ref() {
                        let _ = engine.resize(cols, rows);
                    }
                    session.meta.cols = cols;
                    session.meta.rows = rows;
                }
                drop(map);
                Self::persist(sessions, persistence_path);
            }
            DaemonRequest::Nudge { session_id } => {
                // A freshly attached frontend starts with an empty xterm
                // buffer; output printed before the client connected is gone.
                // Force the session's programs to repaint by resizing to a
                // different size and back: SIGWINCH / window-change fires even
                // when the persisted dimensions already match, so the shell
                // (or tmux pane) redraws into the live client.
                let (engine, cols, rows) = {
                    let map = sessions.lock().unwrap();
                    match map.get(&session_id) {
                        Some(session) => (
                            session.engine.as_ref().map(Arc::clone),
                            session.meta.cols,
                            session.meta.rows,
                        ),
                        None => (None, 0, 0),
                    }
                };
                if let Some(engine) = engine {
                    if rows > 1 {
                        let _ = engine.resize(cols, rows - 1);
                    } else if cols > 1 {
                        let _ = engine.resize(cols - 1, rows);
                    }
                    // Give the remote side a moment to observe the
                    // intermediate size: a zero-gap dance lets a program
                    // read the winsize after the restore and see no change.
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    let _ = engine.resize(cols, rows);
                }
            }
            DaemonRequest::Kill { session_id } => {
                let mut map = sessions.lock().unwrap();
                if let Some(session) = map.get_mut(&session_id) {
                    if let Some(engine) = session.engine.take() {
                        let _ = engine.kill();
                    }
                }
                map.remove(&session_id);
                drop(map);
                Self::persist(sessions, persistence_path);
            }
            DaemonRequest::ListSessions => {
                if let Some(tx) = client_tx {
                    let map = sessions.lock().unwrap();
                    let list: Vec<SessionMeta> = map.values().map(|s| s.meta.clone()).collect();
                    let _ = tx.send(DaemonEvent::SessionList { sessions: list });
                }
            }
            DaemonRequest::AttachAll => {
                if let Some(tx) = client_tx {
                    let map = sessions.lock().unwrap();
                    let list: Vec<SessionMeta> = map.values().map(|s| s.meta.clone()).collect();
                    for meta in &list {
                        let _ = tx.send(DaemonEvent::StateSnapshot {
                            session_id: meta.session_id.clone(),
                            is_busy: meta.is_busy,
                            title: meta.title.clone(),
                        });
                    }
                    let _ = tx.send(DaemonEvent::SessionList { sessions: list });
                }
            }
            DaemonRequest::ProcessList { session_id } => {
                // Clone the engine Arc under the lock, then probe outside it:
                // a remote probe can take seconds and must not block the
                // daemon's other requests.
                let engine = {
                    let map = sessions.lock().unwrap();
                    map.get(&session_id)
                        .and_then(|s| s.engine.as_ref().map(Arc::clone))
                };
                let processes = match engine {
                    Some(engine) => engine.probe_processes().await,
                    None => Vec::new(),
                };
                if let Some(tx) = client_tx {
                    let _ = tx.send(DaemonEvent::ProcessList {
                        session_id,
                        processes,
                    });
                }
            }
            DaemonRequest::Version { .. } => {
                if let Some(tx) = client_tx {
                    let _ = tx.send(DaemonEvent::Version {
                        token: env!("AGENT_IDE_DAEMON_TOKEN").to_string(),
                    });
                }
            }
        }
    }


    /// Live conversation id recorded by the SessionStart marker hook for this
    /// terminal's pinned agent. `.conversation` (resume/clear/compact)
    /// wins over `.startup` (last process the agent started). Markers that
    /// carry a dead pid (opencode) are treated as absent — opencode
    /// conversations are not pinned, so a stale id would resume the wrong
    /// session.
    fn live_conversation_id(&self, session_id: &str) -> Option<String> {
        for ext in ["conversation", "startup"] {
            let path = self.marker_dir.join(format!("{session_id}.{ext}"));
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Some((id, agent, pid)) = parse_marker(&data) {
                    if marker_live(&agent, pid) {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    /// Scan the marker directory and update each session's
    /// `conversation_id` from the agent's SessionStart hook output. Returns
    /// the sessions whose live conversation changed (cleared ones carry
    /// `None`), so the caller can emit `DaemonEvent::Conversation` and
    /// persist. Dead-process markers (opencode) are deleted.
    fn refresh_conversation_markers(&self) -> Vec<(String, Option<String>)> {
        let mut changed: Vec<(String, Option<String>)> = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.marker_dir) else {
            return changed;
        };
        // stem -> (conversation marker, startup id); `.conversation` wins.
        let mut by_pty: HashMap<String, (Option<Marker>, Option<String>)> = HashMap::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if ext != "conversation" && ext != "startup" {
                continue;
            }
            let Some(pty_id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(data) = std::fs::read_to_string(&path) else {
                continue;
            };
            let slot = by_pty.entry(pty_id.to_string()).or_default();
            if ext == "conversation" {
                match parse_marker(&data) {
                    Some((id, agent, pid)) => {
                        if marker_live(&agent, pid) {
                            slot.0 = Some(Marker { id, agent, pid });
                        } else {
                            // Agent process is gone: drop the stale marker.
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                    None => slot.0 = None,
                }
            } else {
                slot.1 = parse_marker(&data).map(|(id, _, _)| id);
            }
        }
        let mut map = self.sessions.lock().unwrap();
        for (pty_id, (conversation, startup)) in by_pty {
            let live = conversation.map(|m| m.id).or(startup);
            let Some(session) = map.get_mut(&pty_id) else {
                continue;
            };
            if session.meta.conversation_id.as_deref() != live.as_deref() {
                changed.push((pty_id.clone(), live.clone()));
                session.meta.conversation_id = live;
            }
        }
        changed
    }

    fn load_sessions(&self) {
        if !self.persistence_path.exists() {
            return;
        }
        let content = std::fs::read_to_string(&self.persistence_path).unwrap_or_default();
        let persisted: Vec<SessionMeta> = serde_json::from_str(&content).unwrap_or_default();
        let mut map = self.sessions.lock().unwrap();
        let mut backfill = Self::now_ms();
        for mut meta in persisted {
            // Legacy rows (created before this field existed) lack a
            // timestamp; give each a distinct value so ordering stays stable.
            if meta.created_at == 0 {
                backfill += 1;
                meta.created_at = backfill;
            }
            let session_id = meta.session_id.clone();
            // Marker (post-switch) wins; otherwise the live conversation is
            // the pinned id.
            let live = self
                .live_conversation_id(&session_id)
                .or_else(|| meta.argv.as_ref().and_then(|argv| pinned_conversation_id(argv)));
            meta.conversation_id = live.clone();
            let live = meta.conversation_id.clone();
            let engine: Option<Arc<dyn PtyEngine>> = if meta.session_type == "local" {
                match self.respawn_local_engine(&meta, live.as_deref()) {
                    Ok(e) => {
                        meta.pgid = e.process_group_id();
                        Some(Arc::new(e))
                    }
                    Err(e) => {
                        error!(session_id = %session_id, error = %e, "failed to respawn persisted local pty session");
                        None
                    }
                }
            } else {
                None
            };
            // A freshly respawned session is idle: the persisted is_busy
            // and agent_active flags belong to the previous daemon run and
            // are not evidence of current activity. The respawned engine
            // re-reports Busy and a foreground agent only once a real
            // process runs. `agent_name` stays sticky as session history.
            meta.is_busy = false;
            meta.agent_active = false;
            map.insert(session_id, DaemonSession { meta, engine, title_busy: false });
        }
    }

    fn respawn_local_engine(
        &self,
        meta: &SessionMeta,
        live_conversation: Option<&str>,
    ) -> Result<LocalPtyEngine, String> {
        LocalPtyEngine::spawn(
            meta.session_id.clone(),
            meta.cwd.clone(),
            meta.cols,
            meta.rows,
            self.event_tx.clone(),
            meta.argv
                .clone()
                .map(|argv| restore_argv(&argv, live_conversation)),
            self.shim_dir.clone(),
        )
    }
    fn respawn_remote_sessions(
        &self,
        project_id: &str,
    ) {
        let sessions = Arc::clone(&self.sessions);
        let persistence_path = self.persistence_path.clone();
        let event_tx = self.event_tx.clone();
        let ssh_manager = Arc::clone(&self.ssh_manager);
        let project_id = project_id.to_string();

        tokio::spawn(async move {
            let to_respawn: Vec<SessionMeta> = {
                let map = sessions.lock().unwrap();
                map.values()
                    .filter(|s| {
                        s.meta.session_type == "ssh"
                            && s.meta.project_id.as_ref() == Some(&project_id)
                            && s.engine.is_none()
                    })
                    .map(|s| s.meta.clone())
                    .collect()
            };

            for meta in to_respawn {
                let ssh_session = match ssh_manager.ensure_connection(&project_id).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!(session_id = %meta.session_id, error = %e, "failed to reconnect SSH for persisted remote session");
                        continue;
                    }
                };

                let session_id = meta.session_id.clone();
                let event_tx = event_tx.clone();
                let persistence_path = persistence_path.clone();
                let sessions = Arc::clone(&sessions);
                let engine = match RemotePtyEngine::spawn(
                    session_id.clone(),
                    meta.cwd.clone(),
                    meta.cols,
                    meta.rows,
                    ssh_session,
                    event_tx.clone(),
                    meta.argv.is_none(),
                    meta.argv.clone().map(|argv| restore_argv(&argv, None)),
                )
                .await
                {
                    Ok(e) => e,
                    Err(e) => {
                        error!(session_id = %session_id, error = %e, "failed to respawn remote pty session");
                        continue;
                    }
                };

                let mut map = sessions.lock().unwrap();
                if let Some(session) = map.get_mut(&session_id) {
                    if session.engine.is_none() {
                        let (latest_cols, latest_rows) = (session.meta.cols, session.meta.rows);
                        if (latest_cols, latest_rows) != (meta.cols, meta.rows) {
                            let _ = engine.resize(latest_cols, latest_rows);
                        }
                        session.engine = Some(Arc::new(engine));
                    }
                    // If an engine is already attached, drop the redundant one;
                    // dropping it closes the channel.
                } else {
                    map.insert(
                        session_id,
                        DaemonSession {
                            meta,
                            engine: Some(Arc::new(engine)),
                            title_busy: false,
                        },
                    );
                }
                drop(map);
                PtyDaemon::persist(&sessions, &persistence_path);
            }
        });
    }

    fn persist(
        sessions: &Arc<Mutex<HashMap<String, DaemonSession>>>,
        persistence_path: &PathBuf,
    ) {
        let map = sessions.lock().unwrap();
        let list: Vec<SessionMeta> = map.values().map(|s| s.meta.clone()).collect();
        drop(map);
        if let Some(parent) = persistence_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&list) {
            let _ = std::fs::write(persistence_path, json);
        }
    }
}

fn basename(path: &str) -> String {
    path.split('/')
        .filter(|s| !s.is_empty())
        .last()
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string())
}


impl PtyDaemon {
    /// Current epoch time in milliseconds (coarse-grained creation clock).
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}
/// True when the title contains a spinner glyph. Coding agents animate
/// braille or dingbat spinner frames in the terminal title while working.
fn title_has_spinner(title: &str) -> bool {
    title.chars().any(|c| {
        matches!(
            c,
            '\u{2800}'..='\u{28FF}' | '\u{25D0}'..='\u{25D3}' | '\u{2733}'..='\u{273D}'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_detection() {
        assert!(title_has_spinner("⠋ fix login bug"));
        assert!(title_has_spinner("✳ refactor terminal store"));
        assert!(!title_has_spinner("marcus@host: ~/Projects/agent-ide"));
        assert!(!title_has_spinner("lib.rs — agent-ide"));
    }

    #[tokio::test]
    async fn spinner_title_drives_busy_state() {
        let (event_tx, event_rx) = mpsc::channel(16);
        let (client_tx, mut client_rx) = mpsc::unbounded_channel();
        let client_cell = Arc::new(std::sync::Mutex::new(Some(client_tx)));
        let sessions = Arc::new(std::sync::Mutex::new(HashMap::new()));
        sessions.lock().unwrap().insert(
            "s1".to_string(),
            DaemonSession {
                meta: SessionMeta {
                    session_id: "s1".to_string(),
                    session_type: "ssh".to_string(),
                    cwd: None,
                    title: "shell".to_string(),
                    is_busy: false,
                    project_id: None,
                    worktree_id: None,
                    agent_name: None,
                    agent_active: false,
                    conversation_id: None,
                    created_at: 0,
                    pgid: None,
                    cols: 80,
                    rows: 24,
                    argv: None,
                },
                engine: None,
                title_busy: false,
            },
        );
        let persistence = tempfile::tempdir().unwrap().path().join("persist.json");

        let broadcaster = tokio::spawn(PtyDaemon::event_broadcaster(
            event_rx,
            sessions,
            client_cell,
            persistence,
        ));

        // Spinner title raises busy.
        event_tx
            .send(("s1".into(), EngineEvent::Title("✳ fix bug".into())))
            .await
            .unwrap();
        // Further spinner frames while busy are plain title updates.
        event_tx
            .send(("s1".into(), EngineEvent::Title("⠋ fix bug".into())))
            .await
            .unwrap();
        // A non-spinner title change clears busy only because the busy was
        // raised by a title.
        event_tx
            .send(("s1".into(), EngineEvent::Title("host:~".into())))
            .await
            .unwrap();
        drop(event_tx);
        broadcaster.await.unwrap();

        let busy = client_rx.recv().await.unwrap();
        assert!(matches!(busy, DaemonEvent::Busy { ref title, .. } if title == "✳ fix bug"));
        let frame = client_rx.recv().await.unwrap();
        assert!(matches!(frame, DaemonEvent::Title { ref title, .. } if title == "⠋ fix bug"));
        let idle = client_rx.recv().await.unwrap();
        assert!(matches!(idle, DaemonEvent::Idle { ref title, .. } if title == "host:~"));
        assert!(client_rx.recv().await.is_none());
    }


    #[tokio::test]
    async fn fg_busy_is_not_cleared_by_title_change() {
        // When busy was raised by the foreground/OSC-133 signal (not a
        // title), a non-spinner title change (e.g. vim setting its title)
        // must NOT clear busy.
        let (event_tx, event_rx) = mpsc::channel(16);
        let (client_tx, mut client_rx) = mpsc::unbounded_channel();
        let client_cell = Arc::new(std::sync::Mutex::new(Some(client_tx)));
        let sessions = Arc::new(std::sync::Mutex::new(HashMap::new()));
        sessions.lock().unwrap().insert(
            "s2".to_string(),
            DaemonSession {
                meta: SessionMeta {
                    session_id: "s2".to_string(),
                    session_type: "local".to_string(),
                    cwd: None,
                    title: "shell".to_string(),
                    is_busy: false,
                    project_id: None,
                    worktree_id: None,
                    agent_name: None,
                    agent_active: false,
                    conversation_id: None,
                    created_at: 0,
                    pgid: None,
                    cols: 80,
                    rows: 24,
                    argv: None,
                },
                engine: None,
                title_busy: false,
            },
        );
        let persistence = tempfile::tempdir().unwrap().path().join("persist.json");

        let broadcaster = tokio::spawn(PtyDaemon::event_broadcaster(
            event_rx,
            sessions,
            client_cell,
            persistence,
        ));

        event_tx
            .send(("s2".into(), EngineEvent::Busy))
            .await
            .unwrap();
        event_tx
            .send(("s2".into(), EngineEvent::Title("main.rs — vim".into())))
            .await
            .unwrap();
        drop(event_tx);
        broadcaster.await.unwrap();

        let busy = client_rx.recv().await.unwrap();
        assert!(matches!(busy, DaemonEvent::Busy { .. }));
        let title = client_rx.recv().await.unwrap();
        assert!(matches!(title, DaemonEvent::Title { ref title, .. } if title == "main.rs — vim"));
        // No Idle event follows: busy stays with the foreground signal.
        assert!(client_rx.recv().await.is_none());
    }
    struct RecordingEngine {
        resizes: std::sync::Mutex<Vec<(u16, u16)>>,
    }

    #[async_trait::async_trait]
    impl PtyEngine for RecordingEngine {
        fn write(&self, _data: &[u8]) -> Result<(), String> {
            Ok(())
        }
        fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
            self.resizes.lock().unwrap().push((cols, rows));
            Ok(())
        }
        fn kill(&self) -> Result<(), String> {
            Ok(())
        }
        fn process_group_id(&self) -> Option<i32> {
            None
        }
    }

    fn nudge_test_meta(session_id: &str) -> SessionMeta {
        SessionMeta {
            session_id: session_id.to_string(),
            session_type: "local".to_string(),
            cwd: None,
            title: "shell".to_string(),
            is_busy: false,
            project_id: None,
            worktree_id: None,
            agent_name: None,
            agent_active: false,
            conversation_id: None,
            created_at: 0,
            pgid: None,
            cols: 80,
            rows: 24,
            argv: None,
        }
    }
    #[tokio::test]
    async fn refresh_conversation_markers_updates_session() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = PtyDaemon::new(dir.path().join("sock"), dir.path().join("persist.json"));
        std::fs::write(
            daemon.marker_dir.join("s9.conversation"),
            r#"{"session_id":"conv-1","source":"resume"}"#,
        )
        .unwrap();
        daemon.sessions.lock().unwrap().insert(
            "s9".to_string(),
            DaemonSession {
                meta: nudge_test_meta("s9"),
                engine: None,
                title_busy: false,
            },
        );

        let changed = daemon.refresh_conversation_markers();
        assert_eq!(
            changed,
            vec![("s9".to_string(), Some("conv-1".to_string()))]
        );
        assert_eq!(
            daemon
                .sessions
                .lock()
                .unwrap()
                .get("s9")
                .unwrap()
                .meta
                .conversation_id
                .as_deref(),
            Some("conv-1")
        );
        // Unchanged markers are not reported again.
        assert!(daemon.refresh_conversation_markers().is_empty());
    }

    #[tokio::test]
    async fn opencode_marker_with_dead_pid_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = PtyDaemon::new(dir.path().join("sock"), dir.path().join("persist.json"));
        // A pid that cannot exist (kernel reserves up to pid_max; use a value
        // far above it on any realistic system).
        std::fs::write(
            daemon.marker_dir.join("s10.conversation"),
            r#"{"session_id":"conv-oc","agent":"opencode","pid":4194304}"#,
        )
        .unwrap();
        daemon.sessions.lock().unwrap().insert(
            "s10".to_string(),
            DaemonSession {
                meta: SessionMeta {
                    conversation_id: Some("conv-oc".to_string()),
                    ..nudge_test_meta("s10")
                },
                engine: None,
                title_busy: false,
            },
        );

        let changed = daemon.refresh_conversation_markers();
        assert_eq!(changed, vec![("s10".to_string(), None)]);
        assert_eq!(
            daemon
                .sessions
                .lock()
                .unwrap()
                .get("s10")
                .unwrap()
                .meta
                .conversation_id,
            None
        );
        // The stale marker file is removed.
        assert!(!daemon.marker_dir.join("s10.conversation").exists());
    }

    #[tokio::test]
    async fn opencode_marker_with_live_pid_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = PtyDaemon::new(dir.path().join("sock"), dir.path().join("persist.json"));
        // This test process is alive, so the marker must be treated as live.
        std::fs::write(
            daemon.marker_dir.join("s11.conversation"),
            format!(
                r#"{{"session_id":"conv-live","agent":"opencode","pid":{}}}"#,
                std::process::id()
            ),
        )
        .unwrap();
        daemon.sessions.lock().unwrap().insert(
            "s11".to_string(),
            DaemonSession {
                meta: nudge_test_meta("s11"),
                engine: None,
                title_busy: false,
            },
        );

        let changed = daemon.refresh_conversation_markers();
        assert_eq!(
            changed,
            vec![("s11".to_string(), Some("conv-live".to_string()))]
        );
    }

    #[tokio::test]
    async fn nudge_resizes_away_and_back_to_force_repaint() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = PtyDaemon::new(
            dir.path().join("sock"),
            dir.path().join("persist.json"),
        );
        let sessions = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (event_tx, _event_rx) = mpsc::channel(16);
        let engine = Arc::new(RecordingEngine {
            resizes: std::sync::Mutex::new(Vec::new()),
        });
        sessions.lock().unwrap().insert(
            "s1".to_string(),
            DaemonSession {
                meta: nudge_test_meta("s1"),
                engine: Some(engine.clone()),
                title_busy: false,
            },
        );
        let ssh_manager = Arc::new(SshManager::new());

        daemon
            .handle_request(
                DaemonRequest::Nudge {
                    session_id: "s1".to_string(),
                },
                &sessions,
                &dir.path().join("persist.json"),
                &event_tx,
                None,
                &ssh_manager,
            )
            .await;

        // Same-size resizes generate no SIGWINCH, so the nudge must step the
        // size away and back to force the shell/tmux to repaint.
        assert_eq!(*engine.resizes.lock().unwrap(), vec![(80, 23), (80, 24)]);
    }

    #[tokio::test]
    async fn nudge_delivers_sigwinch_to_live_session() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let dir = tempfile::tempdir().unwrap();
        let daemon = PtyDaemon::new(
            dir.path().join("sock"),
            dir.path().join("persist.json"),
        );
        let sessions = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (event_tx, mut event_rx) = mpsc::channel(256);
        let ssh_manager = Arc::new(SshManager::new());
        let persistence = dir.path().join("persist.json");

        daemon
            .handle_request(
                DaemonRequest::CreateLocal {
                    session_id: "s1".to_string(),
                    cwd: None,
                    cols: 80,
                    rows: 24,
                    project_id: None,
                    worktree_id: None,
                    argv: Some(vec![
                        "/bin/sh".to_string(),
                        "-c".to_string(),
                        "while :; do stty size; done".to_string(),
                    ]),
                },
                &sessions,
                &persistence,
                &event_tx,
                None,
                &ssh_manager,
            )
            .await;

        // Let the shell boot and start polling; SIGWINCH is not queued.
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        daemon
            .handle_request(
                DaemonRequest::Nudge {
                    session_id: "s1".to_string(),
                },
                &sessions,
                &persistence,
                &event_tx,
                None,
                &ssh_manager,
            )
            .await;

        let deadline = tokio::time::Duration::from_secs(5);
        let mut saw_marker = false;
        while let Ok(ev) = tokio::time::timeout(deadline, event_rx.recv()).await {
            match ev {
                Some((_sid, EngineEvent::Output(data))) => {
                    let bytes = STANDARD.decode(&data).unwrap_or_default();
                    // stty size prints "ROWS COLS"; the nudge shrinks the pty
                    // to 23 rows (and restores it), which the shell observes.
                    if String::from_utf8_lossy(&bytes).contains("23 80") {
                        saw_marker = true;
                        break;
                    }
                }
                Some((_, _)) => {}
                None => break,
            }
        }
        assert!(saw_marker, "nudge must resize the live pty so its programs can observe it");

        daemon
            .handle_request(
                DaemonRequest::Kill {
                    session_id: "s1".to_string(),
                },
                &sessions,
                &persistence,
                &event_tx,
                None,
                &ssh_manager,
            )
            .await;
    }
    #[tokio::test]
    async fn load_sessions_resets_live_flags_keeps_agent_history() {
        let dir = tempfile::tempdir().unwrap();
        let persistence = dir.path().join("persist.json");
        let meta = nudge_test_meta("s1");
        let mut stale = serde_json::to_value(&meta).unwrap();
        stale["isBusy"] = serde_json::Value::Bool(true);
        stale["agentActive"] = serde_json::Value::Bool(true);
        stale["agentName"] = serde_json::Value::String("claude".to_string());
        stale["sessionType"] = serde_json::Value::String("ssh".to_string());
        std::fs::write(
            &persistence,
            serde_json::to_string(&vec![stale]).unwrap(),
        )
        .unwrap();

        let daemon = PtyDaemon::new(dir.path().join("sock"), persistence.clone());
        daemon.load_sessions();
        let map = daemon.sessions.lock().unwrap();
        let restored = map.get("s1").expect("restored session");
        // Live flags belong to the previous daemon run; agent_name is
        // sticky session history.
        assert!(!restored.meta.is_busy);
        assert!(!restored.meta.agent_active);
        assert_eq!(restored.meta.agent_name.as_deref(), Some("claude"));
    }

    #[test]
    fn restore_argv_appends_continue_flag_for_known_agent() {
        let argv = vec![
            "claude".to_string(),
            "--model".to_string(),
            "opus".to_string(),
        ];
        assert_eq!(
            restore_argv(&argv, None),
            vec!["claude", "--model", "opus", "-c"]
        );
    }

    #[test]
    fn restore_argv_uses_exact_resume_when_live_id_known() {
        // omp: fuzzy -c without a marker, exact --resume with one.
        let omp = vec!["omp".to_string()];
        assert_eq!(restore_argv(&omp, None), vec!["omp", "-c"]);
        assert_eq!(
            restore_argv(&omp, Some("conv-omp")),
            vec!["omp", "--resume", "conv-omp"]
        );
        // opencode without a marker falls back to continue-latest like omp.
        let opencode = vec!["opencode".to_string()];
        assert_eq!(restore_argv(&opencode, None), vec!["opencode", "-c"]);
        assert_eq!(
            restore_argv(&opencode, Some("conv-oc")),
            vec!["opencode", "--session", "conv-oc"]
        );
        // pi: exact via --session-id.
        let pi = vec!["pi".to_string(), "--model".to_string(), "opus".to_string()];
        assert_eq!(
            restore_argv(&pi, Some("conv-pi")),
            vec!["pi", "--model", "opus", "--session-id", "conv-pi"]
        );
    }

    #[test]
    fn restore_argv_maps_session_id_to_exact_resume() {
        let argv = vec![
            "claude".to_string(),
            "--model".to_string(),
            "opus".to_string(),
            "--session-id".to_string(),
            "8b1f0e42-1111-2222-3333-444455556666".to_string(),
        ];
        assert_eq!(
            restore_argv(&argv, None),
            vec![
                "claude",
                "--model",
                "opus",
                "--resume",
                "8b1f0e42-1111-2222-3333-444455556666"
            ]
        );
        // A conversation switched inside the agent (SessionStart marker)
        // wins over the pinned id.
        assert_eq!(
            restore_argv(&argv, Some("switched-conv")),
            vec![
                "claude".to_string(),
                "--model".to_string(),
                "opus".to_string(),
                "--resume".to_string(),
                "switched-conv".to_string(),
            ]
        );
        // An explicitly resumed conversation re-runs verbatim.
        let manual = vec![
            "claude".to_string(),
            "--resume".to_string(),
            "abc".to_string(),
        ];
        assert_eq!(restore_argv(&manual, None), manual);
    }

    #[test]
    fn restore_argv_is_idempotent_and_passes_through_unknowns() {
        let resumed = vec!["omp".to_string(), "-c".to_string()];
        assert_eq!(restore_argv(&resumed, None), resumed);
        let shell = vec!["/bin/zsh".to_string(), "-l".to_string()];
        assert_eq!(restore_argv(&shell, None), shell);
        assert!(restore_argv(&[], None).is_empty());
    }

    #[test]
    fn forced_session_id_pins_supported_agents() {
        let argv = vec!["claude".to_string(), "--model".to_string()];
        assert_eq!(
            agents::with_forced_session_id(&argv, "sid", None),
            vec!["claude", "--model", "--session-id", "sid"]
        );
        // pi supports forced ids too, but no --settings flag.
        let pi = vec!["pi".to_string(), "--model".to_string()];
        assert_eq!(
            agents::with_forced_session_id(&pi, "sid", Some(std::path::Path::new("/hooks/h.json"))),
            vec!["pi", "--model", "--session-id", "sid"]
        );
        // The SessionStart hook settings file rides along for claude unless
        // the user passes their own.
        let hooks = std::path::Path::new("/hooks/claude-hooks.json");
        assert_eq!(
            agents::with_forced_session_id(&argv, "sid", Some(hooks)),
            vec![
                "claude",
                "--model",
                "--session-id",
                "sid",
                "--settings",
                "/hooks/claude-hooks.json"
            ]
        );
        // The user's own --settings file suppresses hook injection but the
        // conversation is still pinned.
        let own_settings = vec![
            "claude".to_string(),
            "--settings".to_string(),
            "mine.json".to_string(),
        ];
        assert_eq!(
            agents::with_forced_session_id(&own_settings, "sid", Some(hooks)),
            vec!["claude", "--settings", "mine.json", "--session-id", "sid"]
        );
        // Already pinned or resumed: untouched.
        let pinned = vec!["claude".to_string(), "--session-id".to_string(), "x".to_string()];
        assert_eq!(agents::with_forced_session_id(&pinned, "sid", None), pinned);
        let resumed = vec!["claude".to_string(), "--resume".to_string(), "x".to_string()];
        assert_eq!(agents::with_forced_session_id(&resumed, "sid", None), resumed);
        // omp cannot force ids: untouched.
        let omp = vec!["omp".to_string()];
        assert_eq!(agents::with_forced_session_id(&omp, "sid", None), omp);
    }

    #[tokio::test]
    async fn agent_sighting_seeds_conversation_from_pin() {
        let (event_tx, event_rx) = mpsc::channel(16);
        let (client_tx, _client_rx) = mpsc::unbounded_channel();
        let client_cell = Arc::new(std::sync::Mutex::new(Some(client_tx)));
        let sessions = Arc::new(std::sync::Mutex::new(HashMap::new()));
        sessions.lock().unwrap().insert(
            "s5".to_string(),
            DaemonSession {
                meta: nudge_test_meta("s5"),
                engine: None,
                title_busy: false,
            },
        );
        let persistence = tempfile::tempdir().unwrap().path().join("persist.json");

        let broadcaster = tokio::spawn(PtyDaemon::event_broadcaster(
            event_rx,
            sessions.clone(),
            client_cell,
            persistence,
        ));

        event_tx
            .send((
                "s5".into(),
                EngineEvent::Agent(Some(crate::pty_engine::AgentSighting {
                    name: "claude".to_string(),
                    command: "claude --session-id pinned-9 --settings hooks.json".to_string(),
                })),
            ))
            .await
            .unwrap();
        drop(event_tx);
        broadcaster.await.unwrap();

        let map = sessions.lock().unwrap();
        let meta = &map.get("s5").unwrap().meta;
        // Before any in-app switch, the live conversation is the pinned id.
        assert_eq!(meta.conversation_id.as_deref(), Some("pinned-9"));
    }
    #[tokio::test]
    async fn first_agent_sighting_stores_command_as_argv() {
        let (event_tx, event_rx) = mpsc::channel(16);
        let (client_tx, _client_rx) = mpsc::unbounded_channel();
        let client_cell = Arc::new(std::sync::Mutex::new(Some(client_tx)));
        let sessions = Arc::new(std::sync::Mutex::new(HashMap::new()));
        sessions.lock().unwrap().insert(
            "s3".to_string(),
            DaemonSession {
                meta: nudge_test_meta("s3"),
                engine: None,
                title_busy: false,
            },
        );
        let persistence = tempfile::tempdir().unwrap().path().join("persist.json");

        let broadcaster = tokio::spawn(PtyDaemon::event_broadcaster(
            event_rx,
            sessions.clone(),
            client_cell,
            persistence,
        ));

        event_tx
            .send((
                "s3".into(),
                EngineEvent::Agent(Some(crate::pty_engine::AgentSighting {
                    name: "claude".to_string(),
                    command: "claude --model opus".to_string(),
                })),
            ))
            .await
            .unwrap();
        event_tx
            .send(("s3".into(), EngineEvent::Agent(None)))
            .await
            .unwrap();
        drop(event_tx);
        broadcaster.await.unwrap();

        let map = sessions.lock().unwrap();
        let meta = &map.get("s3").unwrap().meta;
        // The command the user typed becomes the session's restore command;
        // live flag clears with the agent, history persists.
        assert_eq!(
            meta.argv,
            Some(vec!["claude".to_string(), "--model".to_string(), "opus".to_string()])
        );
        assert!(!meta.agent_active);
        assert_eq!(meta.agent_name.as_deref(), Some("claude"));
    }
    #[tokio::test]
    async fn nudge_without_engine_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = PtyDaemon::new(
            dir.path().join("sock"),
            dir.path().join("persist.json"),
        );
        let sessions = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (event_tx, _event_rx) = mpsc::channel(16);
        let ssh_manager = Arc::new(SshManager::new());

        daemon
            .handle_request(
                DaemonRequest::Nudge {
                    session_id: "missing".to_string(),
                },
                &sessions,
                &dir.path().join("persist.json"),
                &event_tx,
                None,
                &ssh_manager,
            )
            .await;
    }
}

