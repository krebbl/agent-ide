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
        if let Some(pos) = buffer[start..]
            .windows(MARKER_PREFIX.len())
            .position(|w| w == MARKER_PREFIX)
        {
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
                if let Some(title) =
                    sanitize_title(&String::from_utf8_lossy(&buffer[title_start..title_end]))
                {
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

// Catppuccin Mocha palette, mirrored from the xterm theme object in
// `src/components/main/TerminalView.tsx`. The backend answers OSC color
// queries with these values; keep both in sync.
const OSC_QUERY_FG: [u8; 3] = [0xcd, 0xd6, 0xf4]; // #cdd6f4
const OSC_QUERY_BG: [u8; 3] = [0x1e, 0x1e, 0x2e]; // #1e1e2e
const OSC_QUERY_CURSOR: [u8; 3] = [0xf5, 0xe0, 0xdc]; // #f5e0dc
const OSC_QUERY_ANSI: [[u8; 3]; 16] = [
    [0x45, 0x47, 0x5a], // 0 black
    [0xf3, 0x8b, 0xa8], // 1 red
    [0xa6, 0xe3, 0xa1], // 2 green
    [0xf9, 0xe2, 0xaf], // 3 yellow
    [0x89, 0xb4, 0xfa], // 4 blue
    [0xf5, 0xc2, 0xe7], // 5 magenta
    [0x89, 0xdc, 0xeb], // 6 cyan
    [0xba, 0xc2, 0xde], // 7 white
    [0x58, 0x5b, 0x70], // 8 bright black
    [0xf3, 0x8b, 0xa8], // 9 bright red
    [0xa6, 0xe3, 0xa1], // 10 bright green
    [0xf9, 0xe2, 0xaf], // 11 bright yellow
    [0x89, 0xb4, 0xfa], // 12 bright blue
    [0xf5, 0xc2, 0xe7], // 13 bright magenta
    [0x89, 0xdc, 0xeb], // 14 bright cyan
    [0xcd, 0xd6, 0xf4], // 15 bright white
];

/// Maximum bytes of a pending (unterminated) sequence to carry across
/// chunks; longer ones are treated as noise.
const OSC_QUERY_CARRY_LIMIT: usize = 8192;

fn osc_query_rgb(color: [u8; 3]) -> String {
    format!(
        "rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}",
        color[0], color[0], color[1], color[1], color[2], color[2]
    )
}

/// The xterm-256color palette entry for `index`: 0-15 from the theme,
/// 16-231 the 6x6x6 color cube, 232-255 the grayscale ramp.
fn osc_query_indexed(index: u8) -> [u8; 3] {
    match index {
        0..=15 => OSC_QUERY_ANSI[index as usize],
        16..=231 => {
            let i = (index - 16) as usize;
            let levels = [0u8, 95, 135, 175, 215, 255];
            [levels[i / 36], levels[(i % 36) / 6], levels[i % 6]]
        }
        _ => {
            let v = 8 + (index - 232) * 10;
            [v, v, v]
        }
    }
}

enum ColorQueryScan {
    NotQuery,
    Partial,
    /// Reply bytes plus the number of bytes consumed from the slice start.
    Query(Vec<u8>, usize),
}

fn parse_color_query(buf: &[u8]) -> ColorQueryScan {
    let mut i = 2; // past ESC ]
    let ident_start = i;
    while i < buf.len() && buf[i].is_ascii_digit() {
        i += 1;
    }
    if i == ident_start {
        return ColorQueryScan::NotQuery;
    }
    if i >= buf.len() {
        // The ident itself may continue in the next chunk.
        return ColorQueryScan::Partial;
    }
    let ident: u32 = std::str::from_utf8(&buf[ident_start..i])
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(u32::MAX);
    if !matches!(ident, 4 | 10 | 11 | 12) || buf[i] != b';' {
        return ColorQueryScan::NotQuery;
    }
    i += 1;
    let payload_start = i;
    let mut end = None;
    while i < buf.len() {
        match buf[i] {
            0x07 | 0x9c => {
                end = Some((i, i + 1));
                break;
            }
            0x1b => match buf.get(i + 1) {
                Some(&b'\\') => {
                    end = Some((i, i + 2));
                    break;
                }
                Some(_) => return ColorQueryScan::NotQuery,
                None => return ColorQueryScan::Partial,
            },
            _ => i += 1,
        }
    }
    let Some((payload_end, consumed)) = end else {
        return ColorQueryScan::Partial;
    };
    let payload = &buf[payload_start..payload_end];
    if !payload.split(|&b| b == b';').any(|p| p == b"?") {
        return ColorQueryScan::NotQuery;
    }

    let mut reply = format!("\x1b]{}", ident);
    match ident {
        10 | 11 | 12 => {
            let color = match ident {
                10 => OSC_QUERY_FG,
                11 => OSC_QUERY_BG,
                _ => OSC_QUERY_CURSOR,
            };
            reply.push_str(&format!(";{}", osc_query_rgb(color)));
        }
        _ => {
            let params: Vec<&[u8]> = payload.split(|&b| b == b';').collect();
            let mut answered = 0;
            let mut k = 0;
            while k + 1 < params.len() {
                if params[k + 1] == b"?" {
                    if let Some(index) =
                        std::str::from_utf8(params[k]).map(str::trim).ok().and_then(|s| s.parse::<u8>().ok())
                    {
                        reply.push_str(&format!(";{};{}", index, osc_query_rgb(osc_query_indexed(index))));
                        answered += 1;
                    }
                }
                k += 2;
            }
            if answered == 0 {
                return ColorQueryScan::NotQuery;
            }
        }
    }
    reply.push_str("\x1b\\");
    ColorQueryScan::Query(reply.into_bytes(), consumed)
}

/// Scan `data` (with `state` carrying partial sequences across chunks) for
/// OSC color queries — `OSC 10/11/12 ; ?` and `OSC 4 ; <index> ; ?` — and
/// return one reply per query, in order. The caller MUST write each reply
/// into the pty input stream immediately: query senders (shell prompt
/// frameworks, tmux, fzf) read the answer synchronously, and a late reply
/// would sit in the input queue until the next interactive program — e.g.
/// `gh auth login` — consumes it and dies on the stale `ESC ]`.
pub fn scan_osc_color_queries(state: &mut Vec<u8>, data: &[u8]) -> Vec<Vec<u8>> {
    let mut buffer = std::mem::take(state);
    buffer.extend_from_slice(data);
    let mut replies = Vec::new();
    let mut pos = 0;

    loop {
        let osc_start =
            match buffer[pos.min(buffer.len())..].windows(2).position(|w| w == b"\x1b]") {
                Some(rel) => pos + rel,
                None => {
                    // Only a lone trailing ESC can be a partial introducer;
                    // a trailing `ESC ]` would have matched the window above.
                    let keep = if buffer.last() == Some(&0x1b) { 1 } else { 0 };
                    state.extend_from_slice(&buffer[buffer.len() - keep..]);
                    break;
                }
            };

        match parse_color_query(&buffer[osc_start..]) {
            ColorQueryScan::NotQuery => pos = osc_start + 2,
            ColorQueryScan::Partial => {
                if buffer.len() - osc_start <= OSC_QUERY_CARRY_LIMIT {
                    state.extend_from_slice(&buffer[osc_start..]);
                }
                break;
            }
            ColorQueryScan::Query(reply, consumed) => {
                replies.push(reply);
                pos = osc_start + consumed;
            }
        }
    }
    replies
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replies(state: &mut Vec<u8>, data: &[u8]) -> Vec<String> {
        scan_osc_color_queries(state, data)
            .into_iter()
            .map(|r| String::from_utf8(r).unwrap())
            .collect()
    }

    #[test]
    fn answers_osc11_query_bel() {
        let mut state = Vec::new();
        assert_eq!(
            replies(&mut state, b"\x1b]11;?\x07"),
            vec!["\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\"]
        );
    }

    #[test]
    fn answers_osc10_and_12_query_st() {
        let mut state = Vec::new();
        assert_eq!(
            replies(&mut state, b"\x1b]10;?\x1b\\"),
            vec!["\x1b]10;rgb:cdcd/d6d6/f4f4\x1b\\"]
        );
        assert_eq!(
            replies(&mut state, b"\x1b]12;?\x07"),
            vec!["\x1b]12;rgb:f5f5/e0e0/dcdc\x1b\\"]
        );
    }

    #[test]
    fn answers_osc4_query_with_theme_and_cube() {
        let mut state = Vec::new();
        assert_eq!(
            replies(&mut state, b"\x1b]4;1;?\x07"),
            vec!["\x1b]4;1;rgb:f3f3/8b8b/a8a8\x1b\\"]
        );
        assert_eq!(
            replies(&mut state, b"\x1b]4;16;?\x07"),
            vec!["\x1b]4;16;rgb:0000/0000/0000\x1b\\"]
        );
        assert_eq!(
            replies(&mut state, b"\x1b]4;17;?\x07"),
            vec!["\x1b]4;17;rgb:0000/0000/5f5f\x1b\\"]
        );
        assert_eq!(
            replies(&mut state, b"\x1b]4;255;?\x07"),
            vec!["\x1b]4;255;rgb:eeee/eeee/eeee\x1b\\"]
        );
    }

    #[test]
    fn answers_multiple_queries_and_mixed_output() {
        let mut state = Vec::new();
        assert_eq!(
            replies(
                &mut state,
                b"prompt \x1b]11;?\x07 mid \x1b]10;?\x1b\\ tail"
            ),
            vec![
                "\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\",
                "\x1b]10;rgb:cdcd/d6d6/f4f4\x1b\\",
            ]
        );
        assert!(state.is_empty());
    }

    #[test]
    fn reassembles_query_split_across_chunks() {
        let mut state = Vec::new();
        assert!(replies(&mut state, b"\x1b]1").is_empty());
        assert!(replies(&mut state, b"1;").is_empty());
        assert_eq!(
            replies(&mut state, b"?\x07"),
            vec!["\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\"]
        );
        assert!(state.is_empty());
    }

    #[test]
    fn reassembles_split_terminator() {
        let mut state = Vec::new();
        assert!(replies(&mut state, b"\x1b]11;?\x1b").is_empty());
        assert_eq!(
            replies(&mut state, b"\\"),
            vec!["\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\"]
        );
        assert!(state.is_empty());
    }

    #[test]
    fn ignores_sets_and_other_osc_sequences() {
        let mut state = Vec::new();
        assert!(replies(&mut state, b"\x1b]11;rgb:ff/00/00\x07").is_empty());
        assert!(replies(&mut state, b"\x1b]0;title\x07").is_empty());
        assert!(replies(&mut state, b"\x1b]133;C\x07").is_empty());
        assert!(replies(&mut state, b"\x1b]8;;http://x\x1b\\").is_empty());
        assert!(replies(&mut state, b"\x1b]4;1;rgb:ff/00/00\x07").is_empty());
        assert!(state.is_empty());
    }

    #[test]
    fn does_not_reply_to_color_set() {
        let mut state = Vec::new();
        assert!(replies(&mut state, b"\x1b]11;#1e1e2e\x07").is_empty());
        assert!(replies(&mut state, b"\x1b]10;rgb:1e1e/1e1e/2e2e\x1b\\").is_empty());
    }

    #[test]
    fn dangling_esc_carries_to_next_chunk() {
        let mut state = Vec::new();
        assert!(replies(&mut state, b"text \x1b").is_empty());
        assert_eq!(
            replies(&mut state, b"]11;?\x07"),
            vec!["\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\"]
        );
        // The consumed tail must not leak into state.
        assert!(state.is_empty());
    }

    #[test]
    fn osc4_multiple_queries_in_one_sequence() {
        let mut state = Vec::new();
        assert_eq!(
            replies(&mut state, b"\x1b]4;1;?;2;?\x07"),
            vec!["\x1b]4;1;rgb:f3f3/8b8b/a8a8;2;rgb:a6a6/e3e3/a1a1\x1b\\"]
        );
    }

    #[test]
    fn malformed_inner_escape_is_skipped() {
        let mut state = Vec::new();
        assert!(replies(&mut state, b"\x1b]11;?x\x1bZ\x07").is_empty());
        assert!(state.is_empty());
    }

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

fn require_pty_client(
    state: &crate::AppState,
) -> Result<Arc<crate::pty_client::PtyClient>, String> {
    state
        .pty_client
        .get()
        .cloned()
        .ok_or_else(|| "PtyClient not initialized".to_string())
}

pub async fn cmd_pty_spawn(
    state: &crate::AppState,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    project_id: Option<String>,
    worktree_id: Option<String>,
    session_type: Option<String>,
    argv: Option<Vec<String>>,
) -> Result<String, String> {
    let pty_client = require_pty_client(state)?;
    let is_remote = session_type.as_deref() == Some("ssh")
        || (project_id.is_some() && session_type.as_deref() != Some("local"));
    let session_id = uuid::Uuid::new_v4().to_string();
    if is_remote {
        pty_client.create_remote(
            session_id.clone(),
            project_id.unwrap_or_default(),
            cwd,
            cols,
            rows,
            worktree_id,
            false,
            argv,
        )?;
    } else {
        pty_client.spawn(
            session_id.clone(),
            cwd,
            cols,
            rows,
            project_id,
            worktree_id,
            argv,
        )?;
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
    argv: Option<Vec<String>>,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<String, String> {
    crate::commands::pty_spawn(
        state.inner().as_ref(),
        cwd,
        cols,
        rows,
        project_id,
        worktree_id,
        session_type,
        argv,
    )
    .await
}

pub async fn cmd_pty_list_sessions(
    state: &crate::AppState,
) -> Result<Vec<crate::pty_protocol::SessionMeta>, String> {
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
    require_pty_client(state)?
        .session_processes(session_id)
        .await
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

pub async fn cmd_pty_nudge(state: &crate::AppState, session_id: String) -> Result<(), String> {
    require_pty_client(state)?.nudge(session_id)
}

#[tauri::command]
pub async fn pty_nudge(
    session_id: String,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<(), String> {
    crate::commands::pty_nudge(state.inner().as_ref(), session_id).await
}

pub async fn cmd_pty_kill(state: &crate::AppState, session_id: String) -> Result<(), String> {
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
    proxy_jump: Option<String>,
) -> Result<(), String> {
    require_pty_client(state)?.register_ssh_project(
        project_id,
        host,
        port,
        username,
        auth_method,
        key_path,
        password,
        proxy_jump,
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
    proxy_jump: Option<String>,
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
        proxy_jump,
    )
    .await
}
