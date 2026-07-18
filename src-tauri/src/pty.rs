use base64::{engine::general_purpose::STANDARD, Engine as _};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Emitter;

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

struct PtySession {
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    _reader_handle: thread::JoinHandle<()>,
    _monitor_handle: thread::JoinHandle<()>,
}

pub struct PtyManager {
    sessions: Mutex<HashMap<String, PtySession>>,
    app_handle: tauri::AppHandle,
}

impl PtyManager {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            app_handle,
        }
    }

    pub fn spawn(&self, cwd: Option<String>, cols: u16, rows: u16) -> Result<String, String> {
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
        if let Some(cwd) = cwd {
            cmd.cwd(cwd);
        }

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

        let session_id = uuid::Uuid::new_v4().to_string();

        let reader_session_id = session_id.clone();
        let reader_app = self.app_handle.clone();
        let reader_handle = thread::spawn(move || {
            let mut reader = master_reader;
            let mut buffer = [0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = STANDARD.encode(&buffer[..n]);
                        let _ = reader_app.emit(
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

        let child_arc = Arc::new(Mutex::new(child));
        let monitor_session_id = session_id.clone();
        let monitor_app = self.app_handle.clone();
        let monitor_child = child_arc.clone();
        let monitor_handle = thread::spawn(move || {
            let mut child = monitor_child.lock().unwrap();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let _ = monitor_app.emit(
                            "pty_exit",
                            PtyExitEvent {
                                session_id: monitor_session_id.clone(),
                                exit_code: Some(status.exit_code() as i32),
                            },
                        );
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
                drop(child);
                thread::sleep(Duration::from_millis(100));
                child = monitor_child.lock().unwrap();
            }
        });

        let session = PtySession {
            child: child_arc,
            writer: Arc::new(Mutex::new(master_writer)),
            master: Arc::new(Mutex::new(master)),
            _reader_handle: reader_handle,
            _monitor_handle: monitor_handle,
        };

        self.sessions
            .lock()
            .unwrap()
            .insert(session_id.clone(), session);
        Ok(session_id)
    }

    pub fn write(&self, session_id: &str, data: &str) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| format!("PTY session {} not found", session_id))?;
        let mut writer = session.writer.lock().unwrap();
        writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Failed to write to PTY: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("Failed to flush PTY: {}", e))?;
        Ok(())
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| format!("PTY session {} not found", session_id))?;
        let master = session.master.lock().unwrap();
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

    pub fn kill(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions
            .remove(session_id)
            .ok_or_else(|| format!("PTY session {} not found", session_id))?;
        let mut child = session.child.lock().unwrap();
        child
            .kill()
            .map_err(|e| format!("Failed to kill PTY: {}", e))?;
        Ok(())
    }

    pub fn kill_all(&self) {
        let mut sessions = self.sessions.lock().unwrap();
        for (_id, session) in sessions.drain() {
            let mut child = session.child.lock().unwrap();
            let _ = child.kill();
        }
    }
}

#[tauri::command]
pub fn pty_spawn(
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    pty_manager: tauri::State<'_, Arc<PtyManager>>,
) -> Result<String, String> {
    pty_manager.spawn(cwd, cols, rows)
}

#[tauri::command]
pub fn pty_write(
    session_id: String,
    data: String,
    pty_manager: tauri::State<'_, Arc<PtyManager>>,
) -> Result<(), String> {
    pty_manager.write(&session_id, &data)
}

#[tauri::command]
pub fn pty_resize(
    session_id: String,
    cols: u16,
    rows: u16,
    pty_manager: tauri::State<'_, Arc<PtyManager>>,
) -> Result<(), String> {
    pty_manager.resize(&session_id, cols, rows)
}

#[tauri::command]
pub fn pty_kill(
    session_id: String,
    pty_manager: tauri::State<'_, Arc<PtyManager>>,
) -> Result<(), String> {
    pty_manager.kill(&session_id)
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
