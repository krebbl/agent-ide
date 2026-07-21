use serde_json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::pty_engine::{EngineEvent, LocalPtyEngine};
use crate::pty_protocol::{DaemonEvent, DaemonRequest, SessionMeta};

struct DaemonSession {
    meta: SessionMeta,
    engine: Option<LocalPtyEngine>,
}

pub struct PtyDaemon {
    socket_path: PathBuf,
    sessions: Arc<Mutex<HashMap<String, DaemonSession>>>,
    persistence_path: PathBuf,
    client_tx: Arc<Mutex<Option<mpsc::UnboundedSender<DaemonEvent>>>>,
    event_tx: mpsc::Sender<(String, EngineEvent)>,
    _event_rx_handle: Option<tokio::task::JoinHandle<()>>,
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

        Self {
            socket_path,
            sessions,
            persistence_path,
            client_tx,
            event_tx,
            _event_rx_handle: Some(event_rx_handle),
        }
    }

    pub async fn run(self) -> Result<(), String> {
        let _ = std::fs::remove_file(&self.socket_path);
        let listener = UnixListener::bind(&self.socket_path)
            .map_err(|e| format!("Failed to bind daemon socket: {}", e))?;
        info!(socket = %self.socket_path.display(), "pty daemon listening");

        self.load_sessions();

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    info!("pty daemon client connected");
                    let sessions = Arc::clone(&self.sessions);
                    let persistence_path = self.persistence_path.clone();
                    let event_tx = self.event_tx.clone();
                    let client_tx_cell = Arc::clone(&self.client_tx);

                    let (client_tx, mut client_rx) = mpsc::unbounded_channel::<DaemonEvent>();
                    *client_tx_cell.lock().unwrap() = Some(client_tx);

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
                                        Self::handle_request(
                                            req,
                                            &sessions,
                                            &persistence_path,
                                            &event_tx,
                                            client_tx_cell.lock().unwrap().as_ref(),
                                        );
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
                EngineEvent::Idle => {
                    if let Some(session) = map.get_mut(&session_id) {
                        if session.meta.is_busy {
                            session.meta.is_busy = false;
                            dirty = true;
                        }
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

    fn handle_request(
        req: DaemonRequest,
        sessions: &Arc<Mutex<HashMap<String, DaemonSession>>>,
        persistence_path: &PathBuf,
        event_tx: &mpsc::Sender<(String, EngineEvent)>,
        client_tx: Option<&mpsc::UnboundedSender<DaemonEvent>>,
    ) {
        match req {
            DaemonRequest::CreateLocal {
                session_id,
                cwd,
                cols,
                rows,
                project_id,
                worktree_id,
            } => {
                let mut map = sessions.lock().unwrap();
                if map.contains_key(&session_id) {
                    warn!(session_id, "session already exists");
                    return;
                }
                drop(map);

                let engine = match LocalPtyEngine::spawn(
                    session_id.clone(),
                    cwd.clone(),
                    cols,
                    rows,
                    event_tx.clone(),
                ) {
                    Ok(e) => e,
                    Err(e) => {
                        if let Some(tx) = client_tx {
                            let _ = tx.send(DaemonEvent::Error { message: e });
                        }
                        return;
                    }
                };

                let title = basename(cwd.as_deref().unwrap_or("~"));
                let meta = SessionMeta {
                    session_id: session_id.clone(),
                    session_type: "local".to_string(),
                    cwd,
                    title: title.clone(),
                    is_busy: false,
                    project_id,
                    worktree_id,
                    cols,
                    rows,
                };

                let mut map = sessions.lock().unwrap();
                map.insert(
                    session_id.clone(),
                    DaemonSession {
                        meta,
                        engine: Some(engine),
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
            DaemonRequest::Write { session_id, data } => {
                use base64::{engine::general_purpose::STANDARD, Engine as _};
                let mut map = sessions.lock().unwrap();
                if let Some(session) = map.get_mut(&session_id) {
                    if let Some(engine) = session.engine.as_ref() {
                        if let Ok(bytes) = STANDARD.decode(&data) {
                            let _ = engine.write(&bytes);
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
                    drop(map);
                    Self::persist(sessions, persistence_path);
                }
            }
            DaemonRequest::Kill { session_id } => {
                let mut map = sessions.lock().unwrap();
                if let Some(mut session) = map.get_mut(&session_id) {
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
        }
    }

    fn load_sessions(&self) {
        if !self.persistence_path.exists() {
            return;
        }
        let content = std::fs::read_to_string(&self.persistence_path).unwrap_or_default();
        let persisted: Vec<SessionMeta> = serde_json::from_str(&content).unwrap_or_default();
        let mut map = self.sessions.lock().unwrap();
        for meta in persisted {
            let session_id = meta.session_id.clone();
            let engine = match self.respawn_engine(&meta) {
                Ok(e) => Some(e),
                Err(e) => {
                    error!(session_id = %session_id, error = %e, "failed to respawn persisted pty session");
                    None
                }
            };
            map.insert(session_id, DaemonSession { meta, engine });
        }
    }

    fn respawn_engine(&self, meta: &SessionMeta) -> Result<LocalPtyEngine, String> {
        LocalPtyEngine::spawn(
            meta.session_id.clone(),
            meta.cwd.clone(),
            meta.cols,
            meta.rows,
            self.event_tx.clone(),
        )
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
