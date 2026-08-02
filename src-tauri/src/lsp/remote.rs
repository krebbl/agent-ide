use russh::ChannelMsg;
use serde_json::Value;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use super::process::{frame_bytes, FrameParser};
use super::registry::ServerSpec;
use super::{dispatch_message, handle_reader_exit, ReaderContext};
use crate::remote_ssh::SessionHandle;

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// SSH exec channels run a non-login, non-interactive shell, so tools
/// installed via rbenv/nvm (initialized in .bashrc/.zshrc) are not on PATH.
/// Probe progressively richer shells to find one that resolves the command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RemoteShell {
    Direct,
    Login,
    Interactive,
}

impl RemoteShell {
    fn wrap(self, inner: &str) -> String {
        match self {
            RemoteShell::Direct => inner.to_string(),
            RemoteShell::Login => format!("$SHELL -lc {}", shell_escape(inner)),
            RemoteShell::Interactive => format!("$SHELL -ilc {}", shell_escape(inner)),
        }
    }
}

struct ExecProbeResult {
    exit_status: Option<u32>,
    stdout: String,
    stderr: String,
}

async fn exec_probe(session: &SessionHandle, command: String) -> ExecProbeResult {
    let mut channel = {
        let handle = session.lock().await;
        match tokio::time::timeout(Duration::from_secs(15), handle.channel_open_session()).await {
            Ok(Ok(ch)) => ch,
            _ => {
                return ExecProbeResult {
                    exit_status: None,
                    stdout: String::new(),
                    stderr: "failed to open SSH channel".to_string(),
                }
            }
        }
    };
    if channel.exec(true, command.into_bytes()).await.is_err() {
        return ExecProbeResult {
            exit_status: None,
            stdout: String::new(),
            stderr: "exec request failed".to_string(),
        };
    }
    let wait = async move {
        let mut stdout = String::new();
        let mut stderr = String::new();
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    if stdout.len() < 4096 {
                        stdout.push_str(&String::from_utf8_lossy(&data));
                    }
                }
                Some(ChannelMsg::ExtendedData { data, ext }) if ext == 1 => {
                    if stderr.len() < 4096 {
                        stderr.push_str(&String::from_utf8_lossy(&data));
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    return ExecProbeResult {
                        exit_status: Some(exit_status),
                        stdout,
                        stderr,
                    }
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                    return ExecProbeResult {
                        exit_status: None,
                        stdout,
                        stderr,
                    }
                }
                _ => {}
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .unwrap_or(ExecProbeResult {
            exit_status: None,
            stdout: String::new(),
            stderr: "probe timed out".to_string(),
        })
}

fn probe_succeeded(result: &ExecProbeResult, command: &str) -> bool {
    if result.exit_status == Some(0) {
        return true;
    }
    // Interactive shells without a tty may never send an exit status;
    // accept a resolved path in stdout as success.
    result
        .stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line == command || line.starts_with('/'))
        .unwrap_or(false)
}

pub async fn remote_resolve_shell(session: &SessionHandle, command: &str) -> Option<RemoteShell> {
    for shell in [
        RemoteShell::Direct,
        RemoteShell::Login,
        RemoteShell::Interactive,
    ] {
        let inner = match shell {
            RemoteShell::Direct => format!("command -v {}", shell_escape(command)),
            _ => format!("command -v {}; exit", shell_escape(command)),
        };
        let probe = shell.wrap(&inner);
        let result = exec_probe(session, probe).await;
        tracing::info!(
            "lsp remote probe: command={} shell={:?} exit={:?} stdout={:?} stderr={:?}",
            command,
            shell,
            result.exit_status,
            result.stdout.trim(),
            result.stderr.trim()
        );
        if probe_succeeded(&result, command) {
            return Some(shell);
        }
    }
    None
}

pub fn build_spawn_command(shell: RemoteShell, spec: &ServerSpec, root_path: &str) -> String {
    let inner = format!(
        "cd {} && exec {} {}",
        shell_escape(root_path),
        spec.command,
        spec.args.join(" ")
    );
    shell.wrap(&inner)
}

pub async fn spawn_remote(
    session: SessionHandle,
    shell: RemoteShell,
    spec: &ServerSpec,
    root_path: &str,
    mut outgoing_rx: mpsc::Receiver<Value>,
    ctx: ReaderContext,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    let mut channel = {
        let handle = session.lock().await;
        tokio::time::timeout(Duration::from_secs(30), handle.channel_open_session())
            .await
            .map_err(|_| "Timed out opening SSH channel".to_string())?
            .map_err(|e| format!("Failed to open SSH channel: {}", e))?
    };

    let command = build_spawn_command(shell, spec, root_path);
    tokio::time::timeout(Duration::from_secs(15), channel.exec(true, command.into_bytes()))
        .await
        .map_err(|_| "exec request timed out".to_string())?
        .map_err(|e| format!("Failed to exec language server: {}", e))?;

    tokio::spawn(async move {
        let mut parser = FrameParser::new();
        loop {
            tokio::select! {
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { data }) => {
                            parser.feed(&data);
                            loop {
                                match parser.next_message() {
                                    Ok(Some(value)) => dispatch_message(&ctx, value).await,
                                    Ok(None) => break,
                                    Err(e) => {
                                        tracing::warn!(
                                            "LSP frame parse error for {}: {}",
                                            ctx.language_id, e
                                        );
                                        break;
                                    }
                                }
                            }
                        }
                        Some(ChannelMsg::ExitStatus { .. }) => break,
                        Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                        _ => {}
                    }
                }
                Some(value) = outgoing_rx.recv() => {
                    match frame_bytes(&value) {
                        Ok(bytes) => {
                            if channel.data(std::io::Cursor::new(bytes)).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("LSP frame encode error: {}", e);
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    let _ = channel.eof().await;
                    let _ = channel.close().await;
                    break;
                }
            }
        }
        handle_reader_exit(ctx).await;
    });

    Ok(())
}

pub async fn remote_command_available(session: &SessionHandle, command: &str) -> bool {
    remote_resolve_shell(session, command).await.is_some()
}
