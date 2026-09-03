use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, trace};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use async_trait::async_trait;
use crate::agent_detect;
use crate::pty::{scan_osc133_command, scan_osc_title};
use crate::pty_protocol::ProcessInfo;

#[async_trait]
pub trait PtyEngine: Send + Sync {
    fn write(&self, data: &[u8]) -> Result<(), String>;
    fn resize(&self, cols: u16, rows: u16) -> Result<(), String>;
    fn kill(&self) -> Result<(), String>;
    /// Process group id of the shell, when the backend can resolve one
    /// (local PTYs). Used to enumerate the terminal's processes.
    fn process_group_id(&self) -> Option<i32>;
    /// Enumerate the processes running in this terminal session. Local
    /// engines run `ps` on this host; remote engines query the server over
    /// SSH. Defaults to empty.
    async fn probe_processes(&self) -> Vec<ProcessInfo> {
        Vec::new()
    }
}

#[async_trait]
impl PtyEngine for LocalPtyEngine {
    fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut writer = self.writer.lock().unwrap();
        writer
            .write_all(data)
            .and_then(|_| writer.flush())
            .map_err(|e| format!("Failed to write to PTY: {}", e))
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let master = self.master.lock().unwrap();
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to resize PTY: {}", e))?;
        #[cfg(unix)]
        if let Some(pgid) = self.shell_pgid {
            let _ = unsafe { libc::kill(-pgid, libc::SIGWINCH) };
        }
        Ok(())
    }

    fn kill(&self) -> Result<(), String> {
        let mut child = self.child.lock().unwrap();
        child.kill().map_err(|e| format!("Failed to kill PTY: {}", e))
    }

    fn process_group_id(&self) -> Option<i32> {
        #[cfg(unix)]
        { self.shell_pgid.map(|p| p as i32) }
        #[cfg(not(unix))]
        { None }
    }

    #[cfg(unix)]
    async fn probe_processes(&self) -> Vec<ProcessInfo> {
        let Some(pgid) = self.process_group_id() else {
            return Vec::new();
        };
        let Some(output) = std::process::Command::new("ps")
            .args(["-A", "-o", "pgid=,pid=,comm=,args="])
            .output()
            .ok()
        else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        agent_detect::parse_processes(&String::from_utf8_lossy(&output.stdout), pgid)
    }
}

/// A foreground coding-agent sighting: the binary name plus the full
/// command line as reported by `ps`.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentSighting {
    pub name: String,
    pub command: String,
}

pub enum EngineEvent {
    Output(String),
    Idle,
    Busy,
    Title(String),
    /// Foreground coding-agent sighting, or `None` when no known agent is
    /// in the foreground.
    Agent(Option<AgentSighting>),
    Exit(Option<i32>),
}

pub struct LocalPtyEngine {
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    shell_pgid: Option<libc::pid_t>,
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
        argv: Option<Vec<String>>,
        shim_dir: Option<std::path::PathBuf>,
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

        let markers_dir = shim_dir
            .as_ref()
            .and_then(|d| d.parent())
            .map(|p| p.join("agent-session-markers"));
        let (cmd, cmd_label) = match &argv {
            Some(argv) if !argv.is_empty() => {
                let mut builder = CommandBuilder::new(&argv[0]);
                builder.args(&argv[1..]);
                // Directly spawned agents (e.g. the built-in launcher) need
                // the conversation id too: the SessionStart marker hook is
                // keyed on it.
                builder.env("AGENT_IDE_CONV_ID", &session_id);
                if let Some(markers) = &markers_dir {
                    builder.env("AGENT_IDE_MARKERS_DIR", markers);
                }
                (builder, argv[0].clone())
            }
            _ => {
                let shell = default_shell();
                let mut builder = CommandBuilder::new(&shell);
                #[cfg(unix)]
                if let Some(dir) = &shim_dir {
                    if let Ok(path) = std::env::var("PATH") {
                        builder.env("PATH", format!("{}:{}", dir.display(), path));
                    }
                    builder.env("AGENT_IDE_CONV_ID", &session_id);
                    if let Some(markers) = &markers_dir {
                        builder.env("AGENT_IDE_MARKERS_DIR", markers);
                    }
                    let zdotdir = dir.join("zdotdir");
                    if zdotdir.join(".zshrc").exists() {
                        let orig = std::env::var("ZDOTDIR")
                            .unwrap_or_else(|_| std::env::var("HOME").unwrap_or_default());
                        builder.env("ZDOTDIR", &zdotdir);
                        builder.env("AGENT_IDE_ORIG_ZDOTDIR", orig);
                    }
                }
                (builder, shell)
            }
        };
        let mut cmd = cmd;
        if let Some(cwd) = &cwd {
            cmd.cwd(cwd);
        }

        // A session started with a non-empty argv has no intervening shell:
        // the spawned command IS the pty's foreground process group leader,
        // so tcgetpgrp() equals shell_pgid for its whole lifetime. Without
        // this flag the monitor never sees a "foreground command" and would
        // never probe that group for a coding agent.
        let direct_cmd = argv.as_ref().is_some_and(|a| !a.is_empty());

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn '{}': {}", cmd_label, e))?;

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
            let mut title_state = Vec::new();
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
                        if let Some(title) = scan_osc_title(&mut title_state, &buffer[..n]) {
                            let _ = reader_event_tx.blocking_send((
                                reader_session_id.clone(),
                                EngineEvent::Title(title),
                            ));
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
        let monitor_direct_cmd = direct_cmd;
        let monitor_handle = thread::spawn(move || {
            info!(session_id = monitor_session_id, "daemon local pty monitor started");
            let mut child = monitor_child.lock().unwrap();
            let mut command_running = false;
            let mut agent_name: Option<String> = None;
            let mut last_agent_probe: Option<Instant> = None;
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
                    } else if (monitor_direct_cmd || fg_pgid != pgid) && !command_running {
                        command_running = true;
                        info!(
                            session_id = monitor_session_id,
                            fg_pgid,
                            shell_pgid = pgid,
                            "foreground command started"
                        );
                        // A foreground command is running in this terminal
                        // regardless of whether it emits OSC-133 markers.
                        // Mark the session busy for the command's whole
                        // duration, not just while it produces output.
                        let _ = monitor_event_tx.blocking_send((
                            monitor_session_id.clone(),
                            EngineEvent::Busy,
                        ));
                    } else if !monitor_direct_cmd && fg_pgid == pgid && command_running {
                        command_running = false;
                        info!(session_id = monitor_session_id, "foreground command finished");
                        if agent_name.is_some() {
                            agent_name = None;
                            last_agent_probe = None;
                            let _ = monitor_event_tx.blocking_send((
                                monitor_session_id.clone(),
                                EngineEvent::Agent(None),
                            ));
                        }
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

                    // Probe the foreground process group for a known coding
                    // agent at most every 500ms while a command is running.
                    let probe_due = last_agent_probe
                        .map(|t| t.elapsed() >= Duration::from_millis(500))
                        .unwrap_or(true);
                    if command_running && probe_due {
                        last_agent_probe = Some(Instant::now());
                        let probe = foreground_agent_sighting(fg_pgid);
                        let probe_name = probe.as_ref().map(|s| s.name.clone());
                        if probe_name != agent_name {
                            agent_name = probe_name;
                            let _ = monitor_event_tx.blocking_send((
                                monitor_session_id.clone(),
                                EngineEvent::Agent(probe),
                            ));
                        }
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
            shell_pgid,
            _reader_handle: reader_handle,
            _monitor_handle: monitor_handle,
        })
    }
}
/// Probe the foreground process group for a known coding agent, returning
/// its name and full command line, or `None` when no known agent runs there.
#[cfg(unix)]
fn foreground_agent_sighting(fg_pgid: i32) -> Option<AgentSighting> {
    let output = std::process::Command::new("ps")
        .args(["-A", "-o", "pgid=,pid=,comm=,args="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    agent_detect::detect_agent_sighting_in(&String::from_utf8_lossy(&output.stdout), fg_pgid)
        .map(|(name, command)| AgentSighting { name, command })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end: spawn a real PTY, run a fake `claude` script in the
    /// foreground, and assert the monitor emits Agent(Some("claude")) while
    /// it runs and Agent(None) once it finishes.
    #[tokio::test]
    #[cfg(unix)]
    async fn detects_agent_in_real_foreground_process() {
        use std::os::unix::fs::PermissionsExt as _;
        use std::time::{Duration, Instant};

        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let claude_path = bin_dir.join("claude");
        let mut f = std::fs::File::create(&claude_path).unwrap();
        // Emulate the real-world shape of an agent CLI: a wrapper whose
        // argv[0] basename is `claude` (Node-wrapped CLIs report comm=node
        // but argv0=/path/to/claude). A plain shebang script would show up
        // as /bin/sh ./bin/claude and match nothing.
        f.write_all(b"#!/bin/sh\nexec -a claude sleep 3\n").unwrap();
        std::fs::set_permissions(&claude_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(64);
        let engine = LocalPtyEngine::spawn(
            "agent-smoke-test".to_string(),
            Some(tmp.path().to_string_lossy().to_string()),
            80,
            24,
            event_tx,
            None,
            None,
        )
        .expect("spawn engine");

        engine
            .write(b"./bin/claude\n")
            .expect("write command");

        let deadline = Instant::now() + Duration::from_secs(15);
        let mut saw_agent = false;
        let mut saw_clear = false;
        let mut saw_command = false;
        while Instant::now() < deadline {
            tokio::select! {
                ev = event_rx.recv() => {
                    let Some((session_id, event)) = ev else { break };
                    if session_id != "agent-smoke-test" {
                        continue;
                    }
                    match event {
                        EngineEvent::Agent(Some(s)) if s.name == "claude" => {
                            saw_agent = true;
                            saw_command = s.command.contains("claude");
                        }
                        EngineEvent::Agent(None) if saw_agent => saw_clear = true,
                        _ => {}
                    }
                    if saw_agent && saw_clear {
                        break;
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(200)) => {}
            }
        }

        engine.kill().ok();

        assert!(saw_agent, "expected Agent(Some(\"claude\")) while script ran");
        assert!(saw_command, "expected sighting to carry the agent command line");
        assert!(saw_clear, "expected Agent(None) after script finished");
    }

    /// Dialog-launched (argv) sessions spawn the agent directly as the pty's
    /// process-group leader, so tcgetpgrp() == shell_pgid for its whole
    /// lifetime. The monitor must treat it as a foreground command and keep
    /// emitting Agent(Some("claude")) instead of never probing it.
    #[tokio::test]
    #[cfg(unix)]
    async fn detects_agent_in_direct_spawned_argv() {
        use std::os::unix::fs::PermissionsExt as _;
        use std::time::{Duration, Instant};

        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let claude_path = bin_dir.join("claude");
        let mut f = std::fs::File::create(&claude_path).unwrap();
        f.write_all(b"#!/bin/sh\nexec -a claude sleep 3\n").unwrap();
        std::fs::set_permissions(&claude_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(64);
        let engine = LocalPtyEngine::spawn(
            "agent-argv-smoke-test".to_string(),
            Some(tmp.path().to_string_lossy().to_string()),
            80,
            24,
            event_tx,
            Some(vec!["claude".to_string()]),
            None,
        )
        .expect("spawn engine");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut saw_agent = false;
        while Instant::now() < deadline {
            tokio::select! {
                ev = event_rx.recv() => {
                    let Some((session_id, event)) = ev else { break };
                    if session_id != "agent-argv-smoke-test" {
                        continue;
                    }
                    if let EngineEvent::Agent(Some(s)) = event {
                        if s.name == "claude" {
                            saw_agent = true;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(200)) => {}
            }
        }

        engine.kill().ok();

        assert!(
            saw_agent,
            "expected Agent(Some(\"claude\")) for a directly-spawned argv session"
        );
    }
}
