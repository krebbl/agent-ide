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

pub async fn spawn_remote(
    session: SessionHandle,
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

    let command = format!(
        "cd {} && exec {} {}",
        shell_escape(root_path),
        spec.command,
        spec.args.join(" ")
    );
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
    let mut channel = {
        let handle = session.lock().await;
        match tokio::time::timeout(Duration::from_secs(15), handle.channel_open_session()).await {
            Ok(Ok(ch)) => ch,
            _ => return false,
        }
    };
    if channel
        .exec(true, format!("command -v {}", shell_escape(command)).into_bytes())
        .await
        .is_err()
    {
        return false;
    }
    let wait = async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::ExitStatus { exit_status }) => return exit_status == 0,
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => return false,
                _ => {}
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .unwrap_or(false)
}
