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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyBusyEvent {
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
                        match scan_osc133_command(&mut osc_state, &buffer[..n]) {
                            Some(Osc133Event::End) => reader_app_state.emit_idle(&reader_session_id),
                            Some(Osc133Event::Start) => reader_app_state.emit_busy(&reader_session_id),
                            None => {}
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
                            match scan_osc133_command(&mut osc_state, data.as_ref()) {
                                Some(Osc133Event::End) => app_state.emit_idle(&session_id),
                                Some(Osc133Event::Start) => app_state.emit_busy(&session_id),
                                None => {}
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
                        Some(ChannelMsg::ExtendedData { data, ext }) if ext == 1 => {
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Osc133Event {
    Start,
    End,
}

pub fn scan_osc133_command(state: &mut Vec<u8>, data: &[u8]) -> Option<Osc133Event> {
    // Look for OSC 133 ; C (command start) or D (command end) followed by BEL (\x07) or ST (ESC \
    const MARKER_PREFIX: &[u8] = b"\x1b]133;";
    let mut buffer = Vec::with_capacity(state.len() + data.len());
    buffer.extend_from_slice(state);
    buffer.extend_from_slice(data);

    let mut result: Option<Osc133Event> = None;
    let mut carry_start: Option<usize> = None;
    let mut start = 0;

    while start + MARKER_PREFIX.len() <= buffer.len() {
        if let Some(pos) = buffer[start..].windows(MARKER_PREFIX.len()).position(|w| w == MARKER_PREFIX) {
            let marker_start = start + pos;
            let cmd_idx = marker_start + MARKER_PREFIX.len();

            let cmd = match buffer.get(cmd_idx) {
                Some(&c) => c,
                None => {
                    carry_start = Some(marker_start);
                    break;
                }
            };

            let term_idx = cmd_idx + 1;

            let mut terminated = false;
            let mut event: Option<Osc133Event> = None;
            if let Some(&b) = buffer.get(term_idx) {
                if b == 0x07 || b == 0x9c {
                    terminated = true;
                    if cmd == b'C' {
                        event = Some(Osc133Event::Start);
                    } else if cmd == b'D' {
                        event = Some(Osc133Event::End);
                    }
                } else if b == 0x1b {
                    match buffer.get(term_idx + 1) {
                        Some(&b'\\') => {
                            terminated = true;
                            if cmd == b'C' {
                                event = Some(Osc133Event::Start);
                            } else if cmd == b'D' {
                                event = Some(Osc133Event::End);
                            }
                        }
                        Some(_) => terminated = true, // non-ST escape, skip this marker
                        None => terminated = false,   // ST may continue in next chunk
                    }
                }
            } else {
                terminated = false;
            }

            if result.is_none() {
                result = event;
            }

            if terminated {
                start = term_idx + 1;
            } else {
                carry_start = Some(marker_start);
                break;
            }
        } else {
            break;
        }
    }

    state.clear();
    if let Some(from) = carry_start {
        state.extend_from_slice(&buffer[from..]);
    } else {
        let keep = buffer.len().min(MARKER_PREFIX.len() - 1);
        state.extend_from_slice(&buffer[buffer.len().saturating_sub(keep)..]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_once(data: &[u8]) -> Option<Osc133Event> {
        let mut state = Vec::new();
        scan_osc133_command(&mut state, data)
    }

    fn scan_split(parts: &[&[u8]]) -> (Option<Osc133Event>, Vec<u8>) {
        let mut state = Vec::new();
        let mut result = None;
        for part in parts {
            if let Some(evt) = scan_osc133_command(&mut state, part) {
                result = Some(evt);
            }
        }
        (result, state)
    }

    #[test]
    fn detects_end_bel() {
        assert_eq!(scan_once(b"\x1b]133;D\x07"), Some(Osc133Event::End));
    }

    #[test]
    fn detects_end_st() {
        assert_eq!(scan_once(b"\x1b]133;D\x1b\\"), Some(Osc133Event::End));
    }

    #[test]
    fn detects_start_bel() {
        assert_eq!(scan_once(b"\x1b]133;C\x07"), Some(Osc133Event::Start));
    }

    #[test]
    fn no_marker() {
        assert_eq!(scan_once(b"hello world"), None);
    }

    #[test]
    fn split_marker_parts() {
        let (result, _) = scan_split(&[b"foo \x1b]133;", b"D\x07 bar"]);
        assert_eq!(result, Some(Osc133Event::End));
    }

    #[test]
    fn split_after_marker_before_bel() {
        let (result, _) = scan_split(&[b"foo \x1b]133;D", b"\x07 bar"]);
        assert_eq!(result, Some(Osc133Event::End));
    }

    #[test]
    fn split_after_marker_before_st() {
        let (result, _) = scan_split(&[b"foo \x1b]133;D", b"\x1b\\ bar"]);
        assert_eq!(result, Some(Osc133Event::End));
    }

    #[test]
    fn split_between_st_bytes() {
        let (result, _) = scan_split(&[b"foo \x1b]133;D\x1b", b"\\ bar"]);
        assert_eq!(result, Some(Osc133Event::End));
    }
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
    worktree_id: Option<String>,
    session_type: Option<String>,
    pty_client: tauri::State<'_, Arc<crate::pty_client::PtyClient>>,
) -> Result<String, String> {
    let is_remote = session_type.as_deref() == Some("ssh")
        || (project_id.is_some() && session_type.as_deref() != Some("local"));
    let session_id = uuid::Uuid::new_v4().to_string();
    if is_remote {
        pty_client.create_remote(session_id.clone(), project_id.unwrap_or_default(), cwd, cols, rows, worktree_id, false)?;
    } else {
        pty_client.spawn(session_id.clone(), cwd, cols, rows, project_id, worktree_id)?;
    }
    Ok(session_id)
}

#[tauri::command]
pub async fn pty_list_sessions(
    pty_client: tauri::State<'_, Arc<crate::pty_client::PtyClient>>,
) -> Result<Vec<crate::pty_protocol::SessionMeta>, String> {
    pty_client.list_sessions().await
}

#[tauri::command]
pub async fn pty_write(
    session_id: String,
    data: String,
    pty_client: tauri::State<'_, Arc<crate::pty_client::PtyClient>>,
) -> Result<(), String> {
    pty_client.write(session_id, data)
}

#[tauri::command]
pub async fn pty_resize(
    session_id: String,
    cols: u16,
    rows: u16,
    pty_client: tauri::State<'_, Arc<crate::pty_client::PtyClient>>,
) -> Result<(), String> {
    pty_client.resize(session_id, cols, rows)
}

#[tauri::command]
pub async fn pty_kill(
    session_id: String,
    pty_client: tauri::State<'_, Arc<crate::pty_client::PtyClient>>,
) -> Result<(), String> {
    pty_client.kill(session_id)
}

#[tauri::command]
pub fn pty_set_active(
    pty_id: Option<String>,
    app_state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<(), String> {
    app_state.set_active_pty(pty_id);
    Ok(())
}

#[tauri::command]
pub async fn pty_register_ssh_project(
    project_id: String,
    host: String,
    port: u16,
    username: String,
    auth_method: String,
    key_path: Option<String>,
    password: Option<String>,
    pty_client: tauri::State<'_, Arc<crate::pty_client::PtyClient>>,
) -> Result<(), String> {
    pty_client.register_ssh_project(
        project_id,
        host,
        port,
        username,
        auth_method,
        key_path,
        password,
    )
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
