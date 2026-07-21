use serde_json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::pty::{
    PtyBusyEvent, PtyExitEvent, PtyIdleEvent, PtyOutputEvent,
};
use crate::pty_protocol::{DaemonEvent, DaemonRequest, SessionMeta};

pub struct PtyClient {
    request_tx: mpsc::UnboundedSender<DaemonRequest>,
    list_waiter: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Vec<SessionMeta>>>>>,
    _read_task: tokio::task::JoinHandle<()>,
}

impl PtyClient {
    pub async fn new(socket_path: PathBuf, app_handle: AppHandle) -> Result<Self, String> {
        ensure_daemon_running(&socket_path).await?;
        let stream = connect_with_retry(&socket_path).await?;
        let (read_half, write_half) = stream.into_split();
        let (request_tx, mut request_rx) = mpsc::unbounded_channel::<DaemonRequest>();

        let mut writer = write_half;
        let write_task = tokio::spawn(async move {
            while let Some(req) = request_rx.recv().await {
                let json = serde_json::to_string(&req).unwrap_or_default();
                if writer.write_all(format!("{}\n", json).as_bytes()).await.is_err() {
                    break;
                }
                let _ = writer.flush().await;
            }
        });

        let list_waiter = Arc::new(Mutex::new(None::<tokio::sync::oneshot::Sender<Vec<SessionMeta>>>));
        let list_waiter_read = Arc::clone(&list_waiter);
        let app_handle_for_read = app_handle.clone();
        let read_task = tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        if let Ok(ev) = serde_json::from_str::<DaemonEvent>(line.trim()) {
                            if let DaemonEvent::SessionList { sessions } = &ev {
                                if let Some(tx) = list_waiter_read.lock().unwrap().take() {
                                    let _ = tx.send(sessions.clone());
                                }
                            }
                            Self::emit_event(&app_handle_for_read, ev);
                        }
                    }
                    Err(e) => {
                        warn!("pty client read error: {}", e);
                        break;
                    }
                }
            }
            info!("pty client read loop ended");
            let _ = write_task.abort();
        });

        Ok(Self {
            request_tx,
            list_waiter,
            _read_task: read_task,
        })
    }

    fn emit_event(app_handle: &AppHandle, ev: DaemonEvent) {
        match ev {
            DaemonEvent::Output { session_id, data } => {
                let _ = app_handle.emit(
                    "pty_output",
                    PtyOutputEvent {
                        session_id,
                        data,
                    },
                );
            }
            DaemonEvent::Idle { session_id, title } => {
                let _ = app_handle.emit("pty_idle", PtyIdleEvent { session_id, title });
            }
            DaemonEvent::Busy { session_id, title } => {
                let _ = app_handle.emit("pty_busy", PtyBusyEvent { session_id, title });
            }
            DaemonEvent::Exit { session_id, exit_code } => {
                let _ = app_handle.emit("pty_exit", PtyExitEvent { session_id, exit_code });
            }
            DaemonEvent::StateSnapshot {
                session_id,
                is_busy,
                title,
            } => {
                let _ = app_handle.emit(
                    "pty_state_snapshot",
                    PtyStateSnapshotEvent {
                        session_id,
                        is_busy,
                        title,
                    },
                );
            }
            DaemonEvent::SessionList { sessions } => {
                let _ = app_handle.emit("pty_session_list", PtySessionListEvent { sessions });
            }
            DaemonEvent::Error { message } => {
                warn!("pty daemon error: {}", message);
            }
            DaemonEvent::Version { .. } => {}
        }
    }

    pub fn spawn(
        &self,
        session_id: String,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        project_id: Option<String>,
        worktree_id: Option<String>,
    ) -> Result<(), String> {
        self.request_tx
            .send(DaemonRequest::CreateLocal {
                session_id,
                cwd,
                cols,
                rows,
                project_id,
                worktree_id,
            })
            .map_err(|_| "pty daemon disconnected".to_string())
    }

    pub fn create_remote(
        &self,
        session_id: String,
        project_id: String,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        worktree_id: Option<String>,
        attach: bool,
    ) -> Result<(), String> {
        self.request_tx
            .send(DaemonRequest::CreateRemote {
                session_id,
                project_id,
                cwd,
                cols,
                rows,
                worktree_id,
                attach,
            })
            .map_err(|_| "pty daemon disconnected".to_string())
    }

    pub fn register_ssh_project(
        &self,
        project_id: String,
        host: String,
        port: u16,
        username: String,
        auth_method: String,
        key_path: Option<String>,
        password: Option<String>,
    ) -> Result<(), String> {
        self.request_tx
            .send(DaemonRequest::RegisterSshProject {
                project_id,
                host,
                port,
                username,
                auth_method,
                key_path,
                password,
            })
            .map_err(|_| "pty daemon disconnected".to_string())
    }

    pub fn write(&self, session_id: String, data: String) -> Result<(), String> {
        self.request_tx
            .send(DaemonRequest::Write { session_id, data })
            .map_err(|_| "pty daemon disconnected".to_string())
    }

    pub fn resize(&self, session_id: String, cols: u16, rows: u16) -> Result<(), String> {
        self.request_tx
            .send(DaemonRequest::Resize {
                session_id,
                cols,
                rows,
            })
            .map_err(|_| "pty daemon disconnected".to_string())
    }

    pub fn kill(&self, session_id: String) -> Result<(), String> {
        self.request_tx
            .send(DaemonRequest::Kill { session_id })
            .map_err(|_| "pty daemon disconnected".to_string())
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionMeta>, String> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Vec<SessionMeta>>();
        *self.list_waiter.lock().unwrap() = Some(tx);
        self.request_tx
            .send(DaemonRequest::ListSessions)
            .map_err(|_| "pty daemon disconnected".to_string())?;
        tokio::time::timeout(tokio::time::Duration::from_secs(5), rx)
            .await
            .map_err(|_| "Timed out waiting for session list".to_string())?
            .map_err(|_| "Session list channel closed".to_string())
    }

    pub fn attach_all(&self) -> Result<(), String> {
        self.request_tx
            .send(DaemonRequest::AttachAll)
            .map_err(|_| "pty daemon disconnected".to_string())
    }
}

const DAEMON_TOKEN: &str = env!("AGENT_IDE_DAEMON_TOKEN");

async fn ensure_daemon_running(socket_path: &PathBuf) -> Result<(), String> {
    if socket_path.exists() {
        if let Ok(is_current) = check_existing_daemon(socket_path).await {
            if is_current {
                return Ok(());
            }
        }
        kill_existing_daemon(socket_path).await;
        let _ = std::fs::remove_file(socket_path);
    }
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Failed to get current executable: {}", e))?;
    let _ = tokio::process::Command::new(current_exe)
        .arg("--pty-daemon")
        .arg("--daemonize")
        .spawn()
        .map_err(|e| format!("Failed to spawn pty daemon: {}", e))?;

    for _ in 0..50 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if socket_path.exists() {
            if UnixStream::connect(socket_path).await.is_ok() {
                return Ok(());
            }
        }
    }
    Err("Timed out waiting for pty daemon".to_string())
}

async fn check_existing_daemon(socket_path: &PathBuf) -> Result<bool, String> {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(|e| format!("Failed to connect to daemon socket: {}", e))?;
    let req = DaemonRequest::Version {
        token: DAEMON_TOKEN.to_string(),
    };
    let json = serde_json::to_string(&req).unwrap_or_default();
    stream
        .write_all(format!("{}\n", json).as_bytes())
        .await
        .map_err(|e| format!("Failed to write version request: {}", e))?;
    stream
        .flush()
        .await
        .map_err(|e| format!("Failed to flush version request: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    match tokio::time::timeout(
        tokio::time::Duration::from_secs(1),
        reader.read_line(&mut line),
    )
    .await
    {
        Ok(Ok(_)) => {
            if let Ok(DaemonEvent::Version { token }) = serde_json::from_str(line.trim()) {
                return Ok(token == DAEMON_TOKEN);
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

async fn kill_existing_daemon(socket_path: &PathBuf) {
    let pid_path = daemon_pid_path();
    let mut killed = false;
    if let Ok(content) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = content.trim().parse::<libc::pid_t>() {
            unsafe {
                let _ = libc::kill(pid, libc::SIGTERM);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
            unsafe {
                let _ = libc::kill(pid, libc::SIGKILL);
            }
            killed = true;
        }
    }
    if !killed {
        if let Ok(output) = tokio::process::Command::new("lsof")
            .arg("-t")
            .arg(socket_path.as_os_str())
            .output()
            .await
        {
            if let Ok(text) = String::from_utf8(output.stdout) {
                for pid_str in text.lines() {
                    if let Ok(pid) = pid_str.trim().parse::<libc::pid_t>() {
                        unsafe {
                            let _ = libc::kill(pid, libc::SIGTERM);
                        }
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_file(pid_path);
}

async fn connect_with_retry(socket_path: &PathBuf) -> Result<UnixStream, String> {
    for _ in 0..10 {
        match UnixStream::connect(socket_path).await {
            Ok(s) => return Ok(s),
            Err(_) => tokio::time::sleep(tokio::time::Duration::from_millis(100)).await,
        }
    }
    Err("Failed to connect to pty daemon".to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyStateSnapshotEvent {
    pub session_id: String,
    pub is_busy: bool,
    pub title: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtySessionListEvent {
    pub sessions: Vec<SessionMeta>,
}

pub fn daemon_config_dir() -> PathBuf {
    dirs::config_dir()
        .expect("config directory is available")
        .join("agent-ide")
}

pub fn daemon_socket_path() -> PathBuf {
    daemon_config_dir().join("pty_daemon.sock")
}

pub fn daemon_pid_path() -> PathBuf {
    daemon_config_dir().join("pty_daemon.pid")
}

pub fn daemon_persistence_path() -> PathBuf {
    daemon_config_dir().join("terminal_sessions.json")
}

fn basename(path: &str) -> String {
    path.split('/')
        .filter(|s| !s.is_empty())
        .last()
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string())
}
