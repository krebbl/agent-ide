use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use russh::{client::Msg, Channel, ChannelMsg};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Emitter;
use tokio::sync::{mpsc, oneshot};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyOutputEvent {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyExitEvent {
    pub session_id: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyIdleEvent {
    pub session_id: String,
    pub title: String,
}

enum PtySession {
    Local(LocalPtySession),
    Remote(RemotePtySession),
}

struct LocalPtySession {
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    _reader_handle: thread::JoinHandle<()>,
    _monitor_handle: thread::JoinHandle<()>,
}

struct RemotePtySession {
    input_tx: mpsc::Sender<String>,
    resize_tx: mpsc::Sender<PtySize>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

pub struct PtyManager {
    sessions: Mutex<HashMap<String, PtySession>>,
    app_handle: tauri::AppHandle,
    app_state: Arc<AppState>,
}

impl PtyManager {
    pub fn new(app_handle: tauri::AppHandle, app_state: Arc<AppState>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            app_handle,
            app_state,
        }
    }

    pub fn set_active_pty(&self, pty_id: Option<String>) {
        self.app_state.set_active_pty(pty_id);
    }

    pub async fn spawn(
        &self,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        project_id: Option<String>,
        session_type: Option<String>,
    ) -> Result<String, String> {
        let is_remote = session_type.as_deref() == Some("ssh")
            || (project_id.is_some() && session_type.as_deref() != Some("local"));

        if is_remote {
            let project_id = project_id.ok_or("SSH session requires a project_id")?;
            self.spawn_remote(cwd, cols, rows, project_id).await
        } else {
            self.spawn_local(cwd, cols, rows)
        }
    }

    fn spawn_local(&self, cwd: Option<String>, cols: u16, rows: u16) -> Result<String, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        let shell = default_shell();
        let mut cmd = CommandBuilder::new(&shell);
        if let Some(ref cwd) = cwd {
            cmd.cwd(cwd);
        }

        let session_id = uuid::Uuid::new_v4().to_string();

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn shell '{}': {}", shell, e))?;

        let master_reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;
        let master_writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to take PTY writer: {}", e))?;
        let master = pair.master;

        let reader_session_id = session_id.clone();
        let reader_app_state = self.app_state.clone();
        let reader_app_handle = self.app_handle.clone();
        let reader_handle = thread::spawn(move || {
            let mut reader = master_reader;
            let mut buffer = [0u8; 4096];
            let mut osc_state = Vec::new();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if contains_osc133_command_end(&mut osc_state, &buffer[..n]) {
                            reader_app_state.emit_idle(&reader_session_id);
                        }
                        let data = STANDARD.encode(&buffer[..n]);
                        let _ = reader_app_handle.emit(
                            "pty_output",
                            PtyOutputEvent {
                                session_id: reader_session_id.clone(),
                                data,
                            },
                        );
                    }
                    Err(_) => break,
                }
            }
        });

        let master_fd = master.as_raw_fd();
        let child_pid = child.process_id().map(|pid| pid as libc::pid_t);
        let shell_pgid = master
            .process_group_leader()
            .map(|pid| pid as libc::pid_t)
            .or_else(|| {
                child_pid.and_then(|pid| {
                    let pgid = unsafe { libc::getpgid(pid) };
                    if pgid < 0 { None } else { Some(pgid) }
                })
            });
        tracing::info!(
            session_id = %session_id,
            master_fd = ?master_fd,
            child_pid = ?child_pid,
            shell_pgid = ?shell_pgid,
            "local pty process group info"
        );

        let child_arc = Arc::new(Mutex::new(child));
        let monitor_session_id = session_id.clone();
        let monitor_app = self.app_handle.clone();
        let monitor_app_state = self.app_state.clone();
        let monitor_child = child_arc.clone();
        let monitor_handle = thread::spawn(move || {
            tracing::info!(session_id = monitor_session_id, "local pty monitor started");
            let mut child = monitor_child.lock().unwrap();
            let mut command_running = false;
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        tracing::info!(
                            session_id = monitor_session_id,
                            exit_code = status.exit_code(),
                            "emitting pty_exit"
                        );
                        let _ = monitor_app.emit(
                            "pty_exit",
                            PtyExitEvent {
                                session_id: monitor_session_id.clone(),
                                exit_code: Some(status.exit_code() as i32),
                            },
                        );
                        break;
                    }
                    Ok(None) => {
                        tracing::trace!(session_id = monitor_session_id, "pty try_wait: still running");
                    }
                    Err(e) => {
                        tracing::error!(session_id = monitor_session_id, error = %e, "pty try_wait failed");
                        break;
                    }
                }

                #[cfg(unix)]
                if let (Some(fd), Some(pgid)) = (master_fd, shell_pgid) {
                    let fg_pgid = unsafe { libc::tcgetpgrp(fd) };
                    if fg_pgid < 0 {
                        let err = std::io::Error::last_os_error();
                        tracing::error!(session_id = monitor_session_id, error = %err, "tcgetpgrp failed");
                    } else if fg_pgid != pgid && !command_running {
                        command_running = true;
                        tracing::info!(
                            session_id = monitor_session_id,
                            fg_pgid,
                            shell_pgid = pgid,
                            "foreground command started"
                        );
                    } else if fg_pgid == pgid && command_running {
                        command_running = false;
                        tracing::info!(session_id = monitor_session_id, "foreground command finished");
                        monitor_app_state.emit_idle(&monitor_session_id);
                    } else {
                        tracing::trace!(
                            session_id = monitor_session_id,
                            fg_pgid,
                            shell_pgid = pgid,
                            command_running,
                            "tcgetpgrp status"
                        );
                    }
                }

                drop(child);
                thread::sleep(Duration::from_millis(100));
                child = monitor_child.lock().unwrap();
            }
            tracing::info!(session_id = monitor_session_id, "local pty monitor ended");
        });

        let session = PtySession::Local(LocalPtySession {
            child: child_arc,
            writer: Arc::new(Mutex::new(master_writer)),
            master: Arc::new(Mutex::new(master)),
            _reader_handle: reader_handle,
            _monitor_handle: monitor_handle,
        });

        self.sessions.lock().unwrap().insert(session_id.clone(), session);
        self.app_state
            .set_pty_title(&session_id, &basename(cwd.as_deref().unwrap_or("~")));
        self.app_state.active_pty_id.lock().unwrap().get_or_insert(session_id.clone());
        Ok(session_id)
    }

    async fn spawn_remote(
        &self,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        project_id: String,
    ) -> Result<String, String> {
        let channel = {
            let connections = self.app_state.ssh_connections.lock().await;
            let conn = connections
                .get(&project_id)
                .ok_or_else(|| format!("No SSH connection for project {}", project_id))?;
            conn.session
                .channel_open_session()
                .await
                .map_err(|e| format!("Failed to open SSH channel: {}", e))?
        };

        let session_id = uuid::Uuid::new_v4().to_string();
        let (input_tx, input_rx) = mpsc::channel::<String>(64);
        let (resize_tx, resize_rx) = mpsc::channel::<PtySize>(16);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let session = PtySession::Remote(RemotePtySession {
            input_tx,
            resize_tx,
            shutdown_tx: Some(shutdown_tx),
        });
        self.sessions.lock().unwrap().insert(session_id.clone(), session);
        self.app_state
            .set_pty_title(&session_id, &basename(cwd.as_deref().unwrap_or("~")));

        let app_handle = self.app_handle.clone();
        let app_state_for_task = self.app_state.clone();
        let session_id_for_task = session_id.clone();
        tauri::async_runtime::spawn(async move {
            run_remote_terminal(
                session_id_for_task.clone(),
                cwd,
                cols,
                rows,
                channel,
                app_handle,
                app_state_for_task,
                input_rx,
                resize_rx,
                shutdown_rx,
            )
            .await;
        });

        Ok(session_id)
    }

    pub async fn write(&self, session_id: &str, data: &str) -> Result<(), String> {
        let session = self
            .sessions
            .lock()
            .unwrap()
            .get(session_id)
            .ok_or_else(|| format!("PTY session {} not found", session_id))?
            .clone_handle_for_write();

        match session {
            WriteHandle::Local(writer) => {
                let mut writer = writer.lock().unwrap();
                writer
                    .write_all(data.as_bytes())
                    .map_err(|e| format!("Failed to write to PTY: {}", e))?;
                writer
                    .flush()
                    .map_err(|e| format!("Failed to flush PTY: {}", e))?;
                Ok(())
            }
            WriteHandle::Remote(input_tx) => {
                input_tx
                    .send(data.to_string())
                    .await
                    .map_err(|e| format!("Failed to send input to remote PTY: {}", e))?;
                Ok(())
            }
        }
    }

    pub async fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let session = self
            .sessions
            .lock()
            .unwrap()
            .get(session_id)
            .ok_or_else(|| format!("PTY session {} not found", session_id))?
            .clone_handle_for_resize();

        match session {
            ResizeHandle::Local(master) => {
                let master = master.lock().unwrap();
                master
                    .resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .map_err(|e| format!("Failed to resize PTY: {}", e))?;
                Ok(())
            }
            ResizeHandle::Remote(resize_tx) => {
                resize_tx
                    .send(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .await
                    .map_err(|e| format!("Failed to send resize to remote PTY: {}", e))?;
                Ok(())
            }
        }
    }

    pub async fn kill(&self, session_id: &str) -> Result<(), String> {
        tracing::info!(session_id, "pty_kill called");
        let session = self
            .sessions
            .lock()
            .unwrap()
            .remove(session_id)
            .ok_or_else(|| format!("PTY session {} not found", session_id))?;
        self.app_state.clear_pty_state(session_id);

        match session {
            PtySession::Local(local) => {
                let mut child = local.child.lock().unwrap();
                child
                    .kill()
                    .map_err(|e| format!("Failed to kill PTY: {}", e))?;
                Ok(())
            }
            PtySession::Remote(mut remote) => {
                if let Some(tx) = remote.shutdown_tx.take() {
                    let _ = tx.send(());
                }
                Ok(())
            }
        }
    }

    pub fn kill_all(&self) {
        let mut sessions = self.sessions.lock().unwrap();
        for (_id, session) in sessions.drain() {
            match session {
                PtySession::Local(local) => {
                    let mut child = local.child.lock().unwrap();
                    let _ = child.kill();
                }
                PtySession::Remote(remote) => {
                    if let Some(tx) = remote.shutdown_tx {
                        let _ = tx.send(());
                    }
                }
            }
        }
    }
}

enum WriteHandle {
    Local(Arc<Mutex<Box<dyn Write + Send>>>), // used by callers
    Remote(mpsc::Sender<String>),
}

enum ResizeHandle {
    Local(Arc<Mutex<Box<dyn MasterPty + Send>>>),
    Remote(mpsc::Sender<PtySize>),
}

impl PtySession {
    fn clone_handle_for_write(&self) -> WriteHandle {
        match self {
            PtySession::Local(local) => WriteHandle::Local(local.writer.clone()),
            PtySession::Remote(remote) => WriteHandle::Remote(remote.input_tx.clone()),
        }
    }

    fn clone_handle_for_resize(&self) -> ResizeHandle {
        match self {
            PtySession::Local(local) => ResizeHandle::Local(local.master.clone()),
            PtySession::Remote(remote) => ResizeHandle::Remote(remote.resize_tx.clone()),
        }
    }
}

async fn run_remote_terminal(
    session_id: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    mut channel: Channel<Msg>,
    app_handle: tauri::AppHandle,
    app_state: Arc<AppState>,
    mut input_rx: mpsc::Receiver<String>,
    mut resize_rx: mpsc::Receiver<PtySize>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut setup_ok = true;

    if let Err(e) = channel
        .request_pty(false, "xterm-256color", cols as u32, rows as u32, 0, 0, &[])
        .await
    {
        let _ = app_handle.emit(
            "pty_exit",
            PtyExitEvent {
                session_id: session_id.clone(),
                exit_code: Some(1),
            },
        );
        tracing::warn!("remote pty request_pty failed: {}", e);
        setup_ok = false;
    }

    if setup_ok {
        if let Err(e) = channel.request_shell(true).await {
            let _ = app_handle.emit(
                "pty_exit",
                PtyExitEvent {
                    session_id: session_id.clone(),
                    exit_code: Some(1),
                },
            );
            tracing::warn!("remote pty request_shell failed: {}", e);
            setup_ok = false;
        }
    }

    if setup_ok {
        let shell_setup = "export PROMPT_COMMAND='printf \"\\033]133;D\\007\"'\n[ -n \"$ZSH_VERSION\" ] && precmd() { printf \"\\033]133;D\\007\"; }\n";
        let _ = channel.data(Cursor::new(shell_setup.as_bytes())).await;

        if let Some(ref cwd) = cwd {
            let cmd = format!("cd {}\n", shell_escape(cwd));
            let _ = channel.data(Cursor::new(cmd.into_bytes())).await;
        }

        let mut exit_code: Option<i32> = None;
        let mut osc_state = Vec::new();

        loop {
            tokio::select! {
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { data }) => {
                            if contains_osc133_command_end(&mut osc_state, data.as_ref()) {
                                app_state.emit_idle(&session_id);
                            }
                            let encoded = STANDARD.encode(data.as_ref());
                            let _ = app_handle.emit(
                                "pty_output",
                                PtyOutputEvent {
                                    session_id: session_id.clone(),
                                    data: encoded,
                                },
                            );
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
                    let _ = channel.data(Cursor::new(data.into_bytes())).await;
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

        tracing::info!(
            session_id = session_id,
            ?exit_code,
            "emitting remote pty_exit"
        );
        let _ = app_handle.emit(
            "pty_exit",
            PtyExitEvent {
                session_id: session_id.clone(),
                exit_code,
            },
        );
    }
}

pub fn contains_osc133_command_end(state: &mut Vec<u8>, data: &[u8]) -> bool {
    // Look for OSC 133 ; D followed by BEL (\x07) or ST (ESC \
    const MARKER: &[u8] = b"\x1b]133;D";
    let mut buffer = Vec::with_capacity(state.len() + data.len());
    buffer.extend_from_slice(state);
    buffer.extend_from_slice(data);

    let mut found = false;
    let mut start = 0;
    while let Some(pos) = buffer[start..].windows(MARKER.len()).position(|w| w == MARKER) {
        let idx = start + pos + MARKER.len();
        let mut terminated = false;
        for &b in &buffer[idx..] {
            if b == 0x07 || b == 0x9c {
                found = true;
                terminated = true;
                break;
            }
            if b == 0x1b {
                // check for ESC \
                if buffer.get(idx + 1) == Some(&b'\\') {
                    found = true;
                }
                terminated = true;
                break;
            }
        }
        if terminated {
            start = idx;
        } else {
            // Marker may be split across chunks; keep scanning from here
            break;
        }
    }

    let keep = buffer.len().min(MARKER.len() - 1);
    state.clear();
    state.extend_from_slice(&buffer[buffer.len().saturating_sub(keep)..]);
    found
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

#[tauri::command]
pub async fn pty_spawn(
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    project_id: Option<String>,
    session_type: Option<String>,
    pty_manager: tauri::State<'_, Arc<PtyManager>>,
) -> Result<String, String> {
    pty_manager
        .spawn(cwd, cols, rows, project_id, session_type)
        .await
}

#[tauri::command]
pub async fn pty_write(
    session_id: String,
    data: String,
    pty_manager: tauri::State<'_, Arc<PtyManager>>,
) -> Result<(), String> {
    pty_manager.write(&session_id, &data).await
}

#[tauri::command]
pub async fn pty_resize(
    session_id: String,
    cols: u16,
    rows: u16,
    pty_manager: tauri::State<'_, Arc<PtyManager>>,
) -> Result<(), String> {
    pty_manager.resize(&session_id, cols, rows).await
}

#[tauri::command]
pub async fn pty_kill(
    session_id: String,
    pty_manager: tauri::State<'_, Arc<PtyManager>>,
) -> Result<(), String> {
    pty_manager.kill(&session_id).await
}

#[tauri::command]
pub fn pty_set_active(
    pty_id: Option<String>,
    pty_manager: tauri::State<'_, Arc<PtyManager>>,
) -> Result<(), String> {
    pty_manager.set_active_pty(pty_id);
    Ok(())
}

fn basename(path: &str) -> String {
    path.split('/').filter(|s| !s.is_empty()).last().map(|s| s.to_string()).unwrap_or_else(|| path.to_string())
}

fn default_shell() -> String {
    #[cfg(target_os = "windows")]
    {
        for exe in ["pwsh.exe", "powershell.exe", "cmd.exe"] {
            if command_exists(exe) {
                return exe.to_string();
            }
        }
        "cmd.exe".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                "/bin/zsh"
            } else {
                "/bin/bash"
            }
            .to_string()
        })
    }
}

#[cfg(target_os = "windows")]
fn command_exists(name: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
}
