use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tracing::{info, trace};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use crate::pty::scan_osc133_command;

pub enum EngineEvent {
    Output(String),
    Idle,
    Busy,
    Exit(Option<i32>),
}

pub struct LocalPtyEngine {
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    _reader_handle: thread::JoinHandle<()>,
    _monitor_handle: thread::JoinHandle<()>,
}

impl LocalPtyEngine {
    pub fn spawn(
        session_id: String,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        event_tx: tokio::sync::mpsc::Sender<(String, EngineEvent)>,
    ) -> Result<Self, String> {
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
        let reader_event_tx = event_tx.clone();
        let reader_handle = thread::spawn(move || {
            let mut reader = master_reader;
            let mut buffer = [0u8; 4096];
            let mut osc_state = Vec::new();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        match scan_osc133_command(&mut osc_state, &buffer[..n]) {
                            Some(crate::pty::Osc133Event::End) => {
                                let _ = reader_event_tx.blocking_send((
                                    reader_session_id.clone(),
                                    EngineEvent::Idle,
                                ));
                            }
                            Some(crate::pty::Osc133Event::Start) => {
                                let _ = reader_event_tx.blocking_send((
                                    reader_session_id.clone(),
                                    EngineEvent::Busy,
                                ));
                            }
                            None => {}
                        }
                        let data = STANDARD.encode(&buffer[..n]);
                        let _ = reader_event_tx.blocking_send((
                            reader_session_id.clone(),
                            EngineEvent::Output(data),
                        ));
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
        info!(
            session_id = %session_id,
            master_fd = ?master_fd,
            child_pid = ?child_pid,
            shell_pgid = ?shell_pgid,
            "daemon local pty process group info"
        );

        let child_arc = Arc::new(Mutex::new(child));
        let monitor_session_id = session_id.clone();
        let monitor_event_tx = event_tx.clone();
        let monitor_child = child_arc.clone();
        let monitor_handle = thread::spawn(move || {
            info!(session_id = monitor_session_id, "daemon local pty monitor started");
            let mut child = monitor_child.lock().unwrap();
            let mut command_running = false;
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        info!(
                            session_id = monitor_session_id,
                            exit_code = status.exit_code(),
                            "emitting pty_exit"
                        );
                        let _ = monitor_event_tx.blocking_send((
                            monitor_session_id.clone(),
                            EngineEvent::Exit(Some(status.exit_code() as i32)),
                        ));
                        break;
                    }
                    Ok(None) => {
                        trace!(session_id = monitor_session_id, "pty try_wait: still running");
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
                        info!(
                            session_id = monitor_session_id,
                            fg_pgid,
                            shell_pgid = pgid,
                            "foreground command started"
                        );
                    } else if fg_pgid == pgid && command_running {
                        command_running = false;
                        info!(session_id = monitor_session_id, "foreground command finished");
                        let _ = monitor_event_tx.blocking_send((
                            monitor_session_id.clone(),
                            EngineEvent::Idle,
                        ));
                    } else {
                        trace!(
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
            info!(session_id = monitor_session_id, "daemon local pty monitor ended");
        });

        Ok(Self {
            child: child_arc,
            writer: Arc::new(Mutex::new(master_writer)),
            master: Arc::new(Mutex::new(master)),
            _reader_handle: reader_handle,
            _monitor_handle: monitor_handle,
        })
    }

    pub fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut writer = self.writer.lock().unwrap();
        writer
            .write_all(data)
            .and_then(|_| writer.flush())
            .map_err(|e| format!("Failed to write to PTY: {}", e))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let master = self.master.lock().unwrap();
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to resize PTY: {}", e))
    }

    pub fn kill(&self) -> Result<(), String> {
        let mut child = self.child.lock().unwrap();
        child.kill().map_err(|e| format!("Failed to kill PTY: {}", e))
    }
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
