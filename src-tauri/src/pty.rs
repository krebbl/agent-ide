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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyTitleEvent {
    pub session_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyAgentEvent {
    pub session_id: String,
    pub name: Option<String>,
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

pub fn scan_osc_title(state: &mut Vec<u8>, data: &[u8]) -> Option<String> {
    fn sanitize_title(title: &str) -> Option<String> {
        let cleaned: String = title
            .chars()
            .filter(|c| !c.is_control() && *c != '\u{FFFD}')
            .collect();
        let trimmed = cleaned.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    // Look for OSC 0 or OSC 2 title sequences: ESC ] 0 ; title BEL/ST
    const MARKER_PREFIX: &[u8] = b"\x1b]";
    let mut buffer = Vec::with_capacity(state.len() + data.len());
    buffer.extend_from_slice(state);
    buffer.extend_from_slice(data);

    let mut result: Option<String> = None;
    let mut carry_start: Option<usize> = None;
    let mut start = 0;

    while start + MARKER_PREFIX.len() <= buffer.len() {
        if let Some(pos) = buffer[start..]
            .windows(MARKER_PREFIX.len())
            .position(|w| w == MARKER_PREFIX)
        {
            let marker_start = start + pos;
            let kind_idx = marker_start + MARKER_PREFIX.len();

            let kind = match buffer.get(kind_idx) {
                Some(&c) => c,
                None => {
                    carry_start = Some(marker_start);
                    break;
                }
            };

            if kind != b'0' && kind != b'2' {
                start = kind_idx + 1;
                continue;
            }

            let semicolon_idx = kind_idx + 1;
            match buffer.get(semicolon_idx) {
                Some(&b';') => {}
                Some(_) => {
                    start = semicolon_idx + 1;
                    continue;
                }
                None => {
                    carry_start = Some(marker_start);
                    break;
                }
            }

            let title_start = semicolon_idx + 1;
            let mut terminated = false;
            let mut title_end = title_start;
            let mut scan = title_start;
            let mut malformed = false;
            while scan < buffer.len() {
                if buffer[scan] == 0x07 || buffer[scan] == 0x9c {
                    terminated = true;
                    title_end = scan;
                    break;
                }
                if buffer[scan] == 0x1b {
                    match buffer.get(scan + 1) {
                        Some(&b'\\') => {
                            terminated = true;
                            title_end = scan;
                            break;
                        }
                        Some(_) => {
                            // Non-ST escape inside title; malformed.
                            malformed = true;
                            start = marker_start + 1;
                            break;
                        }
                        None => {
                            // ESC may be the start of an ST that continues in the next chunk.
                            break;
                        }
                    }
                }
                scan += 1;
            }

            if terminated {
                if let Some(title) = sanitize_title(&String::from_utf8_lossy(&buffer[title_start..title_end])) {
                    result = Some(title);
                }
                start = scan + 1;
            } else if malformed {
                // start already advanced past the malformed marker.
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
        // Keep only a genuine partial marker suffix (ESC or ESC ]).
        let keep = if buffer.ends_with(MARKER_PREFIX) {
            MARKER_PREFIX.len()
        } else if buffer.last() == Some(&MARKER_PREFIX[0]) {
            1
        } else {
            0
        };
        if keep > 0 {
            state.extend_from_slice(&buffer[buffer.len() - keep..]);
        }
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

    fn scan_title_once(data: &[u8]) -> Option<String> {
        let mut state = Vec::new();
        scan_osc_title(&mut state, data)
    }

    fn scan_title_split(parts: &[&[u8]]) -> (Option<String>, Vec<u8>) {
        let mut state = Vec::new();
        let mut result = None;
        for part in parts {
            if let Some(title) = scan_osc_title(&mut state, part) {
                result = Some(title);
            }
        }
        (result, state)
    }

    #[test]
    fn detects_osc0_title_bel() {
        assert_eq!(
            scan_title_once(b"\x1b]0;my title\x07"),
            Some("my title".to_string())
        );
    }

    #[test]
    fn detects_osc2_title_st() {
        assert_eq!(
            scan_title_once(b"\x1b]2;my title\x1b\\"),
            Some("my title".to_string())
        );
    }

    #[test]
    fn ignores_osc1_title() {
        assert_eq!(scan_title_once(b"\x1b]1;icon\x07"), None);
    }

    #[test]
    fn detects_title_split_across_chunks() {
        let (result, _) = scan_title_split(&[b"foo \x1b]0;my", b" title\x07 bar"]);
        assert_eq!(result, Some("my title".to_string()));
    }

    #[test]
    fn detects_title_split_at_st() {
        let (result, _) = scan_title_split(&[b"\x1b]2;my title\x1b", b"\\"]);
        assert_eq!(result, Some("my title".to_string()));
    }

    #[test]
    fn detects_two_sequential_title_calls() {
        let mut state = Vec::new();
        assert_eq!(
            scan_osc_title(&mut state, b"\x1b]0;first\x07"),
            Some("first".to_string())
        );
        assert!(state.is_empty());
        assert_eq!(
            scan_osc_title(&mut state, b"\x1b]0;second\x07"),
            Some("second".to_string())
        );
        assert!(state.is_empty());
    }

    #[test]
    fn detects_last_title_in_single_chunk() {
        let mut state = Vec::new();
        let result = scan_osc_title(&mut state, b"\x1b]0;first\x07\x1b]0;second\x07");
        assert_eq!(result, Some("second".to_string()));
        assert!(state.is_empty());
    }

    #[test]
    fn ignores_title_with_only_invalid_utf8() {
        assert_eq!(scan_title_once(b"\x1b]0;\xff\xfe\x07"), None);
    }

    #[test]
    fn strips_replacement_character_and_controls() {
        assert_eq!(
            scan_title_once(b"\x1b]0;hello \xffworld\x07"),
            Some("hello world".to_string())
        );
    }
}

fn require_pty_client(state: &crate::AppState) -> Result<Arc<crate::pty_client::PtyClient>, String> {
    state.pty_client.get().cloned().ok_or_else(|| "PtyClient not initialized".to_string())
}

pub async fn cmd_pty_spawn(
    state: &crate::AppState,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    project_id: Option<String>,
    worktree_id: Option<String>,
    session_type: Option<String>,
) -> Result<String, String> {
    let pty_client = require_pty_client(state)?;
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
pub async fn pty_spawn(
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    project_id: Option<String>,
    worktree_id: Option<String>,
    session_type: Option<String>,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<String, String> {
    crate::commands::pty_spawn(state.inner().as_ref(), cwd, cols, rows, project_id, worktree_id, session_type).await
}

pub async fn cmd_pty_list_sessions(state: &crate::AppState) -> Result<Vec<crate::pty_protocol::SessionMeta>, String> {
    require_pty_client(state)?.list_sessions().await
}

#[tauri::command]
pub async fn pty_list_sessions(
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<Vec<crate::pty_protocol::SessionMeta>, String> {
    crate::commands::pty_list_sessions(state.inner().as_ref()).await
}

pub async fn cmd_pty_session_processes(
    state: &crate::AppState,
    session_id: String,
) -> Result<Vec<crate::pty_protocol::ProcessInfo>, String> {
    require_pty_client(state)?.session_processes(session_id).await
}

#[tauri::command]
pub async fn pty_session_processes(
    session_id: String,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<Vec<crate::pty_protocol::ProcessInfo>, String> {
    crate::commands::pty_session_processes(state.inner().as_ref(), session_id).await
}

pub async fn cmd_pty_write(
    state: &crate::AppState,
    session_id: String,
    data: String,
) -> Result<(), String> {
    require_pty_client(state)?.write(session_id, data)
}

#[tauri::command]
pub async fn pty_write(
    session_id: String,
    data: String,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<(), String> {
    crate::commands::pty_write(state.inner().as_ref(), session_id, data).await
}

pub async fn cmd_pty_resize(
    state: &crate::AppState,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    require_pty_client(state)?.resize(session_id, cols, rows)
}

#[tauri::command]
pub async fn pty_resize(
    session_id: String,
    cols: u16,
    rows: u16,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<(), String> {
    crate::commands::pty_resize(state.inner().as_ref(), session_id, cols, rows).await
}

pub async fn cmd_pty_kill(
    state: &crate::AppState,
    session_id: String,
) -> Result<(), String> {
    require_pty_client(state)?.kill(session_id)
}

#[tauri::command]
pub async fn pty_kill(
    session_id: String,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<(), String> {
    crate::commands::pty_kill(state.inner().as_ref(), session_id).await
}

pub async fn cmd_pty_set_active(
    state: &crate::AppState,
    pty_id: Option<String>,
) -> Result<(), String> {
    state.set_active_pty(pty_id);
    Ok(())
}

#[tauri::command]
pub async fn pty_set_active(
    pty_id: Option<String>,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<(), String> {
    crate::commands::pty_set_active(state.inner().as_ref(), pty_id).await
}

pub async fn cmd_pty_register_ssh_project(
    state: &crate::AppState,
    project_id: String,
    host: String,
    port: u16,
    username: String,
    auth_method: String,
    key_path: Option<String>,
    password: Option<String>,
) -> Result<(), String> {
    require_pty_client(state)?.register_ssh_project(
        project_id,
        host,
        port,
        username,
        auth_method,
        key_path,
        password,
    )
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
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<(), String> {
    crate::commands::pty_register_ssh_project(
        state.inner().as_ref(),
        project_id,
        host,
        port,
        username,
        auth_method,
        key_path,
        password,
    )
    .await
}


