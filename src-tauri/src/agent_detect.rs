use crate::pty_protocol::ProcessInfo;

/// Binary names of coding agents whose foreground presence makes a terminal
/// session show up in the UI's Active section.
pub const KNOWN_AGENT_BINARIES: &[&str] = &[
    "claude",
    "omp",
    "opencode",
    "codex",
    "gemini",
    "amp",
    "mastracode",
    "pi",
    "copilot",
    "cursor-agent",
];

/// True when `comm` or `argv0_base` names a known agent. `argv0` is
/// normalized to its basename so node-wrapped CLIs (comm=node,
/// argv0=/path/to/claude) match.
pub fn matches_agent(comm: &str, argv0: &str, agents: &[&str]) -> Option<String> {
    let argv0_base = argv0.rsplit('/').next().unwrap_or(argv0);
    agents
        .iter()
        .find(|agent| comm == **agent || argv0_base == **agent)
        .map(|name| name.to_string())
}

/// Parse `ps -A -o pgid=,pid=,comm=,args=` output, keeping only rows whose
/// process group matches `pgid`, annotating agent rows.
pub fn parse_processes(ps_output: &str, pgid: i32) -> Vec<ProcessInfo> {
    let mut procs = Vec::new();
    for line in ps_output.lines() {
        let tokens: Vec<String> = line.split_whitespace().map(|t| t.to_string()).collect();
        if tokens.len() < 3 {
            continue;
        }
        let Ok(row_pgid) = tokens[0].parse::<i32>() else { continue };
        let Ok(pid) = tokens[1].parse::<i32>() else { continue };
        if row_pgid != pgid {
            continue;
        }
        let comm = tokens[2].clone();
        let args = tokens[3..].join(" ");
        let is_agent = is_agent_process(&comm, &args);
        procs.push(ProcessInfo {
            pid,
            pgid: row_pgid,
            comm,
            args,
            is_agent,
        });
    }
    procs
}

/// Scan `ps` output (`pgid=,comm=,args=` columns) for a known agent whose
/// process group matches `pgid`. Matches on comm and argv[0] basename
/// (catches node-wrapped CLIs such as Claude Code).
pub fn detect_agent_in(ps_output: &str, pgid: i32) -> Option<String> {
    detect_agent_in_processes(&parse_processes(ps_output, pgid))
}

/// Find a known agent among already-parsed process rows.
pub fn detect_agent_in_processes(procs: &[ProcessInfo]) -> Option<String> {
    procs.iter().find_map(|p| {
        let argv0 = p.args.split_whitespace().next().unwrap_or("");
        matches_agent(&p.comm, argv0, KNOWN_AGENT_BINARIES)
    })
}

/// True when a single process row was identified as a coding agent.
pub fn is_agent_process(comm: &str, args: &str) -> bool {
    let argv0 = args.split_whitespace().next().unwrap_or("");
    matches_agent(comm, argv0, KNOWN_AGENT_BINARIES).is_some()
}

/// Scan a stream for an invisible identity marker
/// `ESC ] 1338 ; <PREFIX><value> ESC \` (or BEL-terminated) and return the
/// value as a string. `state` carries partial markers across chunks like the
/// OSC scanners.
fn scan_ai_marker_value(state: &mut Vec<u8>, data: &[u8], prefix: &[u8]) -> Option<String> {
    let mut buf = Vec::with_capacity(state.len() + data.len());
    buf.extend_from_slice(state);
    buf.extend_from_slice(data);
    state.clear();

    // Keep only a trailing partial prefix in the carry state: that is the
    // only content that could still be part of a future marker.
    fn keep_tail_partial(state: &mut Vec<u8>, tail: &[u8], prefix: &[u8]) {
        if let Some(pos) = tail.windows(prefix.len()).position(|w| w == prefix) {
            state.extend_from_slice(&tail[pos..]);
        } else {
            let keep = tail.len().min(prefix.len() - 1);
            state.extend_from_slice(&tail[tail.len().saturating_sub(keep)..]);
        }
    }

    let Some(pos) = buf.windows(prefix.len()).position(|w| w == prefix) else {
        keep_tail_partial(state, &buf, prefix);
        return None;
    };

    let value_start = pos + prefix.len();
    let mut end = None;
    for (i, &b) in buf.iter().enumerate().skip(value_start) {
        if b == 0x07 {
            end = Some(i + 1);
            break;
        }
        if b == 0x1b {
            if buf.get(i + 1) == Some(&b'\\') {
                end = Some(i + 2);
            }
            break; // ST or other escape: marker terminates here
        }
    }

    let end = match end {
        Some(e) => e,
        None => {
            // Incomplete marker; keep everything from the marker start.
            state.extend_from_slice(&buf[pos..]);
            return None;
        }
    };

    let value = &buf[value_start..end];
    let value = if value.last() == Some(&0x07) {
        &value[..value.len() - 1]
    } else if value.len() >= 2 && value[value.len() - 2] == 0x1b && value[value.len() - 1] == b'\\'
    {
        &value[..value.len() - 2]
    } else {
        value
    };
    let value = String::from_utf8_lossy(value).trim().to_string();
    keep_tail_partial(state, &buf[end..], prefix);
    if value.is_empty() { None } else { Some(value) }
}

/// Scan for the remote-session terminal marker
/// `ESC ] 1338 ; AI_TTY=<tty> ESC \`, returning the tty name (with any
/// leading `/dev/` stripped, matching `ps`'s tty column).
pub fn scan_ai_tty_marker(state: &mut Vec<u8>, data: &[u8]) -> Option<String> {
    const PREFIX: &[u8] = b"\x1b]1338;AI_TTY=";
    scan_ai_marker_value(state, data, PREFIX).map(|v| {
        v.strip_prefix("/dev/").unwrap_or(&v).to_string()
    })
}

/// Parse `ps -A -o tty=,pid=,comm=,args=` output, keeping only rows whose
/// controlling terminal matches `tty`. Every process attached to a terminal —
/// the shell, foreground and background jobs — shares its tty, so this is a
/// faithful "what's running in this terminal" view without relying on the
/// session leader's pid.
pub fn parse_processes_by_tty(ps_output: &str, tty: &str) -> Vec<ProcessInfo> {
    let mut procs = Vec::new();
    for line in ps_output.lines() {
        let tokens: Vec<String> = line.split_whitespace().map(|t| t.to_string()).collect();
        if tokens.len() < 4 {
            continue;
        }
        if tokens[0] != tty {
            continue;
        }
        let Ok(pid) = tokens[1].parse::<i32>() else { continue };
        let comm = tokens[2].clone();
        let args = tokens[3..].join(" ");
        let is_agent = is_agent_process(&comm, &args);
        procs.push(ProcessInfo {
            pid,
            pgid: 0,
            comm,
            args,
            is_agent,
        });
    }
    procs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_agent_by_comm() {
        assert_eq!(
            matches_agent("claude", "", KNOWN_AGENT_BINARIES),
            Some("claude".to_string())
        );
        assert_eq!(
            matches_agent("omp", "-zsh", KNOWN_AGENT_BINARIES),
            Some("omp".to_string())
        );
    }

    #[test]
    fn matches_agent_by_argv0_basename() {
        // Node-wrapped CLI: comm is "node", argv0 points at the agent bin.
        assert_eq!(
            matches_agent("node", "/usr/local/bin/claude", KNOWN_AGENT_BINARIES),
            Some("claude".to_string())
        );
    }

    #[test]
    fn ignores_non_agent_processes() {
        assert_eq!(matches_agent("zsh", "zsh", KNOWN_AGENT_BINARIES), None);
        assert_eq!(matches_agent("vim", "vim", KNOWN_AGENT_BINARIES), None);
        assert_eq!(matches_agent("node", "npm", KNOWN_AGENT_BINARIES), None);
    }

    #[test]
    fn does_not_flag_npm_or_vite_as_agent() {
        // `npm run dev` spawns node/vite processes; none are agents.
        let output = "  42  100 npm  npm run dev\n  42  101 node /Users/x/.npm/_npx/node_modules/vite/bin/vite.js dev\n  42  102 node /usr/local/bin/npm\n";
        let procs = parse_processes(output, 42);
        assert!(procs.iter().all(|p| !p.is_agent));
        assert_eq!(detect_agent_in(output, 42), None);
    }

    #[test]
    fn flags_real_agent_rows() {
        let output = "  42 100 zsh -zsh\n  42 101 node /usr/local/bin/claude --dangerously-skip-permissions\n";
        let procs = parse_processes(output, 42);
        assert!(procs[1].is_agent);
        assert!(!procs[0].is_agent);
    }

    #[test]
    fn parses_agent_from_ps_output() {
        let output = "  1234 999 zsh    -zsh\n  4321 100 claude claude --dangerously-skip-permissions\n  4321 101 git   git status\n";
        assert_eq!(
            detect_agent_in(output, 4321),
            Some("claude".to_string())
        );
    }

    #[test]
    fn ps_output_other_pgid_ignored() {
        let output = "  4321 100 claude claude\n  9999 200 zsh    -zsh\n";
        assert_eq!(detect_agent_in(output, 9999), None);
    }

    #[test]
    fn ps_output_missing_columns_tolerated() {
        let output = "   -    ?? ?? ???\n  4321 100 claude claude\n";
        assert_eq!(
            detect_agent_in(output, 4321),
            Some("claude".to_string())
        );
    }

    #[test]
    fn filters_to_requested_pgid() {
        let output = "  42  100 zsh    -zsh\n  42  101 claude claude --dangerously-skip-permissions\n  43  102 vim    file.txt\n";
        let procs = parse_processes(output, 42);
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[0].pid, 100);
        assert_eq!(procs[0].comm, "zsh");
        assert_eq!(procs[1].pid, 101);
        assert_eq!(procs[1].comm, "claude");
        assert_eq!(procs[1].args, "claude --dangerously-skip-permissions");
    }

    #[test]
    fn joins_remaining_args() {
        let output = "  7 8 node /usr/local/bin/claude --something\n";
        let procs = parse_processes(output, 7);
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].comm, "node");
        assert_eq!(procs[0].args, "/usr/local/bin/claude --something");
    }

    #[test]
    fn ignores_unparseable_rows() {
        let output = "   -    ?? ???\n  9 10 zsh -zsh\n";
        let procs = parse_processes(output, 9);
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].pid, 10);
    }

    #[test]
    fn detects_detached_agent_among_parsed() {
        let procs = parse_processes("  42 201 zsh -zsh\n  42 202 node /usr/local/bin/claude -p foo\n", 42);
        assert_eq!(
            detect_agent_in_processes(&procs),
            Some("claude".to_string())
        );
    }

    #[test]
    fn ai_tty_marker_single_chunk_st() {
        let mut state = Vec::new();
        assert_eq!(
            scan_ai_tty_marker(&mut state, b"prompt \x1b]1338;AI_TTY=ttys002\x1b\\ done"),
            Some("ttys002".to_string())
        );
        // Trailing content is retained only as a bounded possible prefix.
        assert!(state.len() <= b"\x1b]1338;AI_TTY=".len() - 1);
    }

    #[test]
    fn ai_tty_marker_bel_terminated() {
        let mut state = Vec::new();
        assert_eq!(
            scan_ai_tty_marker(&mut state, b"\x1b]1338;AI_TTY=pts/1\x07"),
            Some("pts/1".to_string())
        );
        assert!(state.len() <= b"\x1b]1338;AI_TTY=".len() - 1);
    }

    #[test]
    fn ai_tty_marker_strips_dev_prefix() {
        let mut state = Vec::new();
        assert_eq!(
            scan_ai_tty_marker(&mut state, b"\x1b]1338;AI_TTY=/dev/ttys007\x1b\\"),
            Some("ttys007".to_string())
        );
    }

    #[test]
    fn ai_tty_marker_split_across_chunks() {
        let mut state = Vec::new();
        assert_eq!(scan_ai_tty_marker(&mut state, b"\x1b]1338;AI_TTY="), None);
        assert_eq!(
            scan_ai_tty_marker(&mut state, b"pts/3\x1b\\"),
            Some("pts/3".to_string())
        );
        assert!(state.is_empty());
    }

    #[test]
    fn ai_tty_marker_absent_keeps_bounded_state() {
        let mut state = Vec::new();
        assert_eq!(scan_ai_tty_marker(&mut state, b"plain output text"), None);
        assert!(state.len() <= b"\x1b]1338;AI_TTY=".len() - 1);
        assert_eq!(scan_ai_tty_marker(&mut state, b" more"), None);
        assert!(state.len() <= b"\x1b]1338;AI_TTY=".len() - 1);
    }

    #[test]
    fn filters_processes_by_tty() {
        let output = "  ttys002  100 zsh    -zsh\n  ttys002  101 claude claude --dangerously-skip-permissions\n  ttys003  200 vim    file.txt\n";
        let procs = parse_processes_by_tty(output, "ttys002");
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[0].pid, 100);
        assert_eq!(procs[0].comm, "zsh");
        assert_eq!(procs[1].pid, 101);
        assert_eq!(procs[1].comm, "claude");
        assert_eq!(procs[1].args, "claude --dangerously-skip-permissions");
    }

    #[test]
    fn filters_by_pts_tty() {
        let output = "  pts/1  300 zsh -zsh\n  pts/2  301 zsh -zsh\n";
        let procs = parse_processes_by_tty(output, "pts/2");
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].pid, 301);
    }
}