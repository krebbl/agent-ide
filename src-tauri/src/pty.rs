use serde::Serialize;
use std::sync::Arc;

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


