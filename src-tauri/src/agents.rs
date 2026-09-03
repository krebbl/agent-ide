use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptTransport {
    Argv,
    Stdin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModel {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    pub id: String,
    pub label: String,
    pub description: String,
    pub command: Vec<String>,
    pub prompt_transport: PromptTransport,
    pub enabled: bool,
    pub include_in_default_presets: bool,
    /// Flag used to select the model, e.g. "--model".
    pub model_flag: String,
    /// When set, the initial prompt is passed via this flag instead of as a
    /// positional argument (e.g. opencode's "--prompt").
    pub prompt_flag: Option<String>,
    /// Curated model catalogue. `list_agent_models` may enrich this list at
    /// runtime (e.g. `opencode models`).
    pub models: Vec<AgentModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub id: String,
    pub label: String,
    pub description: String,
    pub command: Vec<String>,
    pub prompt_transport: String,
    pub enabled: bool,
    pub installed: bool,
    pub binary_path: Option<String>,
}

pub fn builtin_agents() -> Vec<AgentDefinition> {
    vec![
        AgentDefinition {
            id: "claude".to_string(),
            label: "Claude".to_string(),
            description: "Anthropic's coding agent for reading code, editing files, and running terminal workflows.".to_string(),
            command: vec!["claude".to_string(), "--dangerously-skip-permissions".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: true,
            model_flag: "--model".to_string(),
            prompt_flag: None,
            models: vec![
                AgentModel { id: "opus".to_string(), label: "Claude Opus 4.5".to_string() },
                AgentModel { id: "sonnet".to_string(), label: "Claude Sonnet 4.5".to_string() },
                AgentModel { id: "haiku".to_string(), label: "Claude Haiku 4.5".to_string() },
            ],
        },
        AgentDefinition {
            id: "omp".to_string(),
            label: "OMP".to_string(),
            description: "Oh My Pi — the terminal-native coding agent harness (omp).".to_string(),
            command: vec!["omp".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: true,
            model_flag: "--model".to_string(),
            prompt_flag: None,
            models: vec![
                AgentModel { id: "opus".to_string(), label: "Opus (fuzzy)".to_string() },
                AgentModel { id: "sonnet".to_string(), label: "Sonnet (fuzzy)".to_string() },
                AgentModel { id: "haiku".to_string(), label: "Haiku (fuzzy)".to_string() },
                AgentModel { id: "gpt-5.2".to_string(), label: "GPT-5.2 (fuzzy)".to_string() },
                AgentModel { id: "deepseek-v4-flash".to_string(), label: "DeepSeek V4 Flash (fuzzy)".to_string() },
            ],
        },
        AgentDefinition {
            id: "amp".to_string(),
            label: "Amp".to_string(),
            description: "Amp's coding agent for terminal-first coding, subagents, and task work.".to_string(),
            command: vec!["amp".to_string()],
            prompt_transport: PromptTransport::Stdin,
            enabled: true,
            include_in_default_presets: false,
            model_flag: "--model".to_string(),
            prompt_flag: None,
            models: vec![],
        },
        AgentDefinition {
            id: "codex".to_string(),
            label: "Codex".to_string(),
            description: "OpenAI's coding agent for reading, modifying, and running code across tasks.".to_string(),
            command: vec!["codex".to_string(), "--dangerously-bypass-approvals-and-sandbox".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: true,
            model_flag: "--model".to_string(),
            prompt_flag: None,
            models: vec![],
        },
        AgentDefinition {
            id: "gemini".to_string(),
            label: "Gemini".to_string(),
            description: "Google's terminal agent for coding, problem-solving, and task work.".to_string(),
            command: vec!["gemini".to_string(), "--approval-mode=auto_edit".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: true,
            model_flag: "--model".to_string(),
            prompt_flag: None,
            models: vec![],
        },
        AgentDefinition {
            id: "mastracode".to_string(),
            label: "Mastracode".to_string(),
            description: "Mastra's coding agent for building, debugging, and shipping code from the terminal.".to_string(),
            command: vec!["mastracode".to_string()],
            prompt_transport: PromptTransport::Stdin,
            enabled: true,
            include_in_default_presets: false,
            model_flag: "--model".to_string(),
            prompt_flag: None,
            models: vec![],
        },
        AgentDefinition {
            id: "opencode".to_string(),
            label: "OpenCode".to_string(),
            description: "Open-source AI coding agent with full file and shell access by default.".to_string(),
            command: vec!["opencode".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: false,
            model_flag: "--model".to_string(),
            prompt_flag: Some("--prompt".to_string()),
            // The model catalogue is fetched live via `opencode models`.
            models: vec![],
        },
        AgentDefinition {
            id: "pi".to_string(),
            label: "Pi".to_string(),
            description: "Minimal terminal coding harness.".to_string(),
            command: vec!["pi".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: false,
            model_flag: "--model".to_string(),
            prompt_flag: None,
            models: vec![],
        },
        AgentDefinition {
            id: "copilot".to_string(),
            label: "Copilot".to_string(),
            description: "GitHub Copilot agent for terminal-based coding tasks.".to_string(),
            command: vec!["copilot".to_string(), "--allow-tool=write".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: false,
            model_flag: "--model".to_string(),
            prompt_flag: None,
            models: vec![],
        },
        AgentDefinition {
            id: "cursor-agent".to_string(),
            label: "Cursor Agent".to_string(),
            description: "Cursor's coding agent that prompts for every action.".to_string(),
            command: vec!["cursor-agent".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: false,
            model_flag: "--model".to_string(),
            prompt_flag: None,
            models: vec![],
        },
    ]
}

pub fn check_agent_ready(id: &str) -> Option<AgentStatus> {
    let agent = builtin_agents().into_iter().find(|a| a.id == id)?;
    let binary_name = agent.command.first()?;
    let binary_path = find_real_binary(binary_name);
    Some(agent_to_status(agent, binary_path))
}

pub fn check_all_agents_ready() -> Vec<AgentStatus> {
    let agents = builtin_agents();
    // Binary lookup can fall back to a login shell (~1.4s each). Run all
    // lookups in parallel so N agents cost one shell latency worst case.
    std::thread::scope(|scope| {
        let handles: Vec<_> = agents
            .into_iter()
            .map(|agent| {
                scope.spawn(move || {
                    let binary_path = agent
                        .command
                        .first()
                        .and_then(|name| find_real_binary(name));
                    agent_to_status(agent, binary_path)
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

fn agent_to_status(agent: AgentDefinition, binary_path: Option<PathBuf>) -> AgentStatus {
    AgentStatus {
        id: agent.id,
        label: agent.label,
        description: agent.description,
        command: agent.command,
        prompt_transport: match agent.prompt_transport {
            PromptTransport::Argv => "argv".to_string(),
            PromptTransport::Stdin => "stdin".to_string(),
        },
        enabled: agent.enabled,
        installed: binary_path.is_some(),
        binary_path: binary_path.map(|p| p.to_string_lossy().to_string()),
    }
}

/// Build the full argv for launching an agent with an optional model and the
/// initial prompt. For Stdin-transport agents the prompt is omitted; the
/// caller is responsible for delivering it via stdin.
pub fn launch_command(
    agent_id: &str,
    model: Option<&str>,
    prompt: &str,
) -> Result<Vec<String>, String> {
    let agent = builtin_agents()
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| format!("Unknown agent: {}", agent_id))?;

    let mut argv = agent.command;
    if let Some(m) = model {
        if !m.trim().is_empty() {
            argv.push(agent.model_flag);
            argv.push(m.to_string());
        }
    }
    if agent.prompt_transport == PromptTransport::Argv {
        if let Some(flag) = agent.prompt_flag {
            argv.push(flag);
            argv.push(prompt.to_string());
        } else {
            argv.push(prompt.to_string());
        }
    }
    Ok(argv)
}

/// Exact-resume flag: resumes a SPECIFIC conversation by id, for agents
/// whose CLI supports it (verified per `--help`). Agents without exact
/// resume return `None`.
pub fn exact_resume_flag(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude" => Some("--resume"),
        "pi" => Some("--session-id"),
        "omp" => Some("--resume"),
        "opencode" => Some("--session"),
        _ => None,
    }
}

/// Flag that resumes an agent's most recent conversation in its working
/// directory — the fuzzy fallback when no conversation id is known.
/// Agents without resume support return `None`.
pub fn resume_flag(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude" | "omp" | "opencode" => Some("-c"),
        _ => None,
    }
}

/// Agents whose CLI can force the session id of a new conversation. Only
/// these get a per-terminal PATH shim that pins the conversation to the
/// terminal's session id; the others keep fuzzy restore. `settings_flag`
/// marks CLIs that understand claude's `--settings <file>` for hook
/// injection.
const SESSION_ID_SHIM_AGENTS: &[(&str, &str, bool)] =
    &[("claude", "--session-id", true), ("pi", "--session-id", false)];

/// True when the user already pins or resumes a conversation, so nothing
/// may be injected.
pub(crate) fn pins_conversation(argv: &[String]) -> bool {
    argv.iter().any(|a| {
        matches!(
            a.as_str(),
            "-r" | "--resume" | "-c" | "--continue" | "--session-id"
        ) || a.starts_with("--resume=")
            || a.starts_with("--session-id=")
    })
}

/// Pin a directly-spawned agent command to the terminal session's id with
/// the agent's pin flag, so a reboot can resume that exact conversation.
/// Claude additionally gets the SessionStart hook settings file so
/// conversation switches made inside the agent (e.g. `/resume`) are
/// recorded. No-op for non-agent argv, agents without forced-id support,
/// and commands that already pin or resume a conversation.
pub fn with_forced_session_id(
    agent_argv: &[String],
    session_id: &str,
    hook_settings: Option<&Path>,
) -> Vec<String> {
    let mut out = agent_argv.to_vec();
    let Some(binary) = out.first() else {
        return out;
    };
    let known = crate::agent_detect::matches_agent(
        binary,
        binary,
        crate::agent_detect::KNOWN_AGENT_BINARIES,
    );
    let Some((_, pin_flag, settings_flag)) = known
        .as_deref()
        .and_then(|id| SESSION_ID_SHIM_AGENTS.iter().find(|(id2, _, _)| *id2 == id))
    else {
        return out;
    };
    if !pins_conversation(&out) {
        out.push(pin_flag.to_string());
        out.push(session_id.to_string());
    }
    if *settings_flag {
        if let Some(settings) = hook_settings {
            if !out.iter().any(|a| a == "--settings" || a.starts_with("--settings=")) {
                out.push("--settings".to_string());
                out.push(settings.display().to_string());
            }
        }
    }
    out
}

/// Create the PATH shims directory: per-agent wrappers that run the real
/// binary with `--session-id $AGENT_IDE_CONV_ID` unless the user already pins
/// or resumes a conversation, plus a Claude Code SessionStart hook settings
/// file that records conversation switches (see `session-marker.sh` below).
/// Only agents whose CLI supports forced session ids get a shim. Rewritten on
/// every daemon start so content updates propagate to existing installs.
pub fn ensure_session_id_shims(
    dir: &std::path::Path,
    marker_dir: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::create_dir_all(marker_dir)?;
    let hooks_json = dir.join("claude-hooks.json");
    for (agent, pin_flag, settings_flag) in SESSION_ID_SHIM_AGENTS {
        let shim = dir.join(agent);
        let Some(real) = find_real_binary(agent) else {
            continue;
        };
        // Claude-only: pass the SessionStart hook settings file so switches
        // made inside the agent are recorded.
        let settings_inject = if *settings_flag {
            format!(" --settings \"{}\"", hooks_json.display())
        } else {
            String::new()
        };
        let settings_guard = if *settings_flag {
            "        --settings|--settings=*)\n            exec \"{real}\" \"$@\"\n            ;;\n"
        } else {
            ""
        };
        let script = format!(
            "#!/bin/sh\n# agent-ide per-terminal wrapper: pin the conversation to this\n# terminal's session id unless the user already resumes or pins one.{settings_guard_0}for arg in \"$@\"; do\n    case \"$arg\" in\n        -r|--resume|-c|--continue|--session-id|--resume=*|--session-id=*)\n            exec \"{real}\"{settings_inject} \"$@\"\n            ;;\n    esac\ndone\nif [ -n \"$AGENT_IDE_CONV_ID\" ]; then\n    exec \"{real}\" {pin_flag} \"$AGENT_IDE_CONV_ID\"{settings_inject} \"$@\"\nfi\nexec \"{real}\" \"$@\"\n",
            settings_guard_0 = settings_guard,
            real = real.display(),
            settings_inject = settings_inject,
            pin_flag = pin_flag
        );
        std::fs::write(&shim, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    // SessionStart marker hook: the hook receives the live conversation id
    // on stdin, so a switch made inside the agent (`/resume`, `/clear`,
    // `/compact`) survives a reboot. Startup events land in a lower-priority
    // `.startup` file so one-shot `claude -p` runs cannot clobber the
    // authoritative `.conversation` marker. No-op outside agent-ide
    // terminals (guarded on AGENT_IDE_CONV_ID).
    let marker_script = dir.join("session-marker.sh");
    let marker_script_body = format!(
        "#!/bin/sh\n# agent-ide SessionStart marker: records the live conversation of the\n# terminal pinned by AGENT_IDE_CONV_ID so a reboot resumes the conversation\n# the user actually switched to. Startup events land in a lower-priority\n# .startup file (one-shot `claude -p` runs also fire startup); explicit\n# conversation moves (resume, clear, compact) win via .conversation.\n[ -n \"$AGENT_IDE_CONV_ID\" ] || exit 0\ninput=$(cat)\nsource=$(printf '%s' \"$input\" | sed -n 's/.*\"source\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p' | head -n 1)\nif [ \"$source\" = \"startup\" ]; then\n    printf '%s' \"$input\" > \"{marker_dir}/$AGENT_IDE_CONV_ID.startup\"\nelse\n    printf '%s' \"$input\" > \"{marker_dir}/$AGENT_IDE_CONV_ID.conversation\"\nfi\n",
        marker_dir = marker_dir.display()
    );
    std::fs::write(&marker_script, marker_script_body)?;
    let hooks_settings = serde_json::json!({
        "hooks": {
            "SessionStart": [
                { "hooks": [ { "type": "command", "command": quoted_command(&marker_script) } ] }
            ]
        }
    })
    .to_string();
    std::fs::write(&hooks_json, hooks_settings)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&marker_script, std::fs::Permissions::from_mode(0o755))?;
    }
    // PATH shims only catch invocations of the agent binary itself. Wrappers
    // (caveman claude, npm scripts, aliases) exec the real binary directly,
    // so the SessionStart hook must also be registered in the user's global
    // claude settings. The marker script no-ops without AGENT_IDE_CONV_ID,
    // so claude outside agent-ide terminals is unaffected.
    let _ = ensure_global_claude_hook(&marker_script);
    let _ = ensure_agent_marker_hooks();
    // A ZDOTDIR wrapper sources the user's own zsh config and then defines
    // per-agent wrapper functions. Functions take precedence over PATH
    // lookup, so rc files or plugins that rebuild PATH cannot bypass them.
    let zdotdir = dir.join("zdotdir");
    std::fs::create_dir_all(&zdotdir)?;
    let shim_dir = dir.display();
    let zshenv = format!(
        "# agent-ide: chain the user's zshenv while terminal shims are active.\nif [ -n \"$AGENT_IDE_ORIG_ZDOTDIR\" ] && [ -f \"$AGENT_IDE_ORIG_ZDOTDIR/.zshenv\" ]; then\n    . \"$AGENT_IDE_ORIG_ZDOTDIR/.zshenv\"\nelif [ -f \"$HOME/.zshenv\" ]; then\n    . \"$HOME/.zshenv\"\nfi\n"
    );
    let mut wrappers = String::new();
    for (agent, pin_flag, settings_flag) in SESSION_ID_SHIM_AGENTS {
        let settings_inject = if *settings_flag {
            format!(" --settings \"{}\"", hooks_json.display())
        } else {
            String::new()
        };
        let settings_guard = if *settings_flag {
            "        --settings|--settings=*)\n            command {agent} \"$@\"; return ;;\n"
        } else {
            ""
        };
        wrappers.push_str(&format!(
            "{agent}() {{\n    local arg\n{settings_guard_0}    for arg in \"$@\"; do\n        case \"$arg\" in\n            -r|--resume|-c|--continue|--session-id|--resume=*|--session-id=*)\n                command {agent}{settings_inject} \"$@\"; return ;;\n        esac\n    done\n    if [ -n \"$AGENT_IDE_CONV_ID\" ]; then\n        command {agent} {pin_flag} \"$AGENT_IDE_CONV_ID\"{settings_inject} \"$@\"\n    else\n        command {agent} \"$@\"\n    fi\n}}\n",
            agent = agent,
            settings_guard_0 = settings_guard,
            settings_inject = settings_inject,
            pin_flag = pin_flag
        ));
    }
    let zshrc = format!(
        "# agent-ide: chain the user's zshrc, then put terminal shims on PATH\n# and pin agent conversations to this terminal's session id.\nif [ -n \"$AGENT_IDE_ORIG_ZDOTDIR\" ] && [ -f \"$AGENT_IDE_ORIG_ZDOTDIR/.zshrc\" ]; then\n    . \"$AGENT_IDE_ORIG_ZDOTDIR/.zshrc\"\nelif [ -f \"$HOME/.zshrc\" ]; then\n    . \"$HOME/.zshrc\"\nfi\ncase \":$PATH:\" in\n    *\":{shim_dir}:\"*) ;;\n    *) export PATH=\"{shim_dir}:$PATH\" ;;\nesac\n{wrappers}",
        shim_dir = shim_dir,
        wrappers = wrappers
    );
    std::fs::write(zdotdir.join(".zshenv"), zshenv)?;
    std::fs::write(zdotdir.join(".zshrc"), zshrc)?;
    Ok(())
}

/// Session marker hooks for agents without claude's `--settings` flag.
/// pi and omp are pi-family agents: a minimal TypeScript extension (their
/// native lifecycle-hook mechanism) reacts to `session_start` — fired on
/// startup, `/new`, `/resume` and `/fork`. opencode gets a plugin reacting
/// to `session.created`/`session.idle`. All write the same
/// `<pty>.conversation` marker JSON the daemon already reads, keyed by
/// env vars the terminal exports; they no-op outside agent-ide terminals.
fn ensure_agent_marker_hooks() -> std::io::Result<()> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(());
    };
    const PI_OMP_EXTENSION: &str = r#"// agent-ide session marker: records the live conversation id of this
// terminal so a reboot resumes exactly it. No-op outside agent-ide
// terminals (guarded on AGENT_IDE_CONV_ID).
import { mkdirSync, writeFileSync } from "node:fs";

export default function (pi: any) {
  pi.on("session_start", async (_event: any, ctx: any) => {
    const conv = process.env.AGENT_IDE_CONV_ID;
    const dir = process.env.AGENT_IDE_MARKERS_DIR;
    if (!conv || !dir || !ctx?.sessionManager) return;
    const file: string = ctx.sessionManager.getSessionFile?.() ?? "";
    if (!file) return;
    // Session files are named <timestamp>_<uuid>; the UUID is the session
    // id `--resume` (omp) and `--session-id` (pi) accept.
    const base = file.split("/").pop() ?? file;
    const id = base.split("_").pop()?.replace(/\.jsonl$/, "") ?? "";
    if (!id) return;
    try { mkdirSync(dir, { recursive: true }); } catch {}
    try {
      writeFileSync(`${dir}/${conv}.conversation`, JSON.stringify({ session_id: id }));
    } catch {}
  });
}
"#;
    const OPENCODE_PLUGIN: &str = r#"// agent-ide session marker: records the live conversation id of this
// terminal so a reboot resumes exactly it. No-op outside agent-ide
// terminals (guarded on AGENT_IDE_CONV_ID).
import { mkdirSync, writeFileSync } from "node:fs";

export const AgentIdeSessionMarker = async () => {
  return {
    event: async ({ event }) => {
      const conv = process.env.AGENT_IDE_CONV_ID;
      const dir = process.env.AGENT_IDE_MARKERS_DIR;
      if (!conv || !dir) return;
      if (event.type !== "session.created" && event.type !== "session.idle") return;
      const props = event.properties ?? {};
      const id = props.info?.id ?? props.info?.sessionID ?? props.sessionID;
      if (!id) return;
      try { mkdirSync(dir, { recursive: true }); } catch {}
      try {
        writeFileSync(`${dir}/${conv}.conversation`, JSON.stringify({ session_id: id }));
      } catch {}
    },
  };
};
"#;
    for (agent, rel_path, content) in [
        ("pi", ".pi/agent/extensions/agent-ide-session-marker.ts", PI_OMP_EXTENSION),
        ("omp", ".omp/agent/extensions/agent-ide-session-marker.ts", PI_OMP_EXTENSION),
        ("opencode", ".config/opencode/plugins/agent-ide-session-marker.js", OPENCODE_PLUGIN),
    ] {
        if find_real_binary(agent).is_none() {
            continue;
        }
        let path = home.join(rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    Ok(())
}

/// Hook commands run through a shell: quote the path so spaces (e.g.
/// "Application Support") survive.
fn quoted_command(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

/// Register the SessionStart marker hook in the user's global
/// `~/.claude/settings.json` so it also fires for agents launched through
/// wrappers that bypass the PATH shims. Idempotent: an existing entry with
/// the same command is left alone, everything else in the file is preserved.
fn ensure_global_claude_hook(marker_script: &Path) -> std::io::Result<()> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(());
    };
    let settings_path = home.join(".claude").join("settings.json");
    let mut settings: serde_json::Value = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !settings.is_object() {
        return Ok(());
    }
    // The previously written registration may be unquoted (older agent-ide);
    // treat both spellings as already-registered.
    let command = quoted_command(marker_script);
    let command_unquoted = marker_script.display().to_string();
    {
        let hooks = settings
            .as_object_mut()
            .unwrap()
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}));
        if !hooks.is_object() {
            return Ok(());
        }
        let session_start = hooks
            .as_object_mut()
            .unwrap()
            .entry("SessionStart")
            .or_insert_with(|| serde_json::json!([]));
        let Some(entries) = session_start.as_array_mut() else {
            return Ok(());
        };
        let already_registered = entries.iter().any(|entry| {
            entry
                .get("hooks")
                .and_then(|h| h.as_array())
                .is_some_and(|hs| {
                    hs.iter().any(|h| {
                        matches!(
                            h.get("command").and_then(|c| c.as_str()),
                            Some(c) if c == command.as_str() || c == command_unquoted.as_str()
                        )
                    })
                })
        });
        if already_registered {
            // Upgrade an unquoted legacy entry to the quoted form, then fall
            // through to the write below.
            for entry in entries.iter_mut() {
                for h in entry
                    .get_mut("hooks")
                    .and_then(|h| h.as_array_mut())
                    .into_iter().flatten()
                {
                    if h.get("command").and_then(|c| c.as_str()) == Some(command_unquoted.as_str()) {
                        if let Some(obj) = h.as_object_mut() {
                            obj.insert("command".to_string(), serde_json::json!(command));
                        }
                    }
                }
            }
        } else {
            entries.push(serde_json::json!({
                "hooks": [ { "type": "command", "command": command } ]
            }));
        }
    }
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
    Ok(())
}
/// Model catalogue for an agent. Curated defaults, enriched at runtime where
/// the CLI exposes the authoritative list (`opencode models`, `omp models`).
pub fn list_agent_models(agent_id: &str) -> Vec<AgentModel> {
    let agent = match builtin_agents().into_iter().find(|a| a.id == agent_id) {
        Some(a) => a,
        None => return Vec::new(),
    };
    match agent.id.as_str() {
        "opencode" => opencode_models().unwrap_or_default(),
        "omp" => omp_models().unwrap_or(agent.models),
        _ => agent.models,
    }
}

fn opencode_models() -> Option<Vec<AgentModel>> {
    let output = Command::new("opencode").arg("models").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let models: Vec<AgentModel> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| line.contains('/'))
        .map(|line| AgentModel {
            id: line.to_string(),
            label: line.to_string(),
        })
        .collect();
    if models.is_empty() {
        None
    } else {
        Some(models)
    }
}

#[derive(Deserialize)]
struct OmpModelsResponse {
    #[serde(default)]
    models: Vec<OmpModelEntry>,
}

#[derive(Deserialize)]
struct OmpModelEntry {
    #[serde(default)]
    selector: String,
    #[serde(default)]
    name: String,
}

/// Full model catalogue from `omp models --json`. The `selector` is the value
/// omp's `--model` flag accepts. The config default (which may use a :high
/// suffix the catalogue does not contain) is prepended as its own entry.
fn omp_models() -> Option<Vec<AgentModel>> {
    let output = Command::new("omp")
        .args(["models", "--json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let parsed: OmpModelsResponse = serde_json::from_slice(&output.stdout).ok()?;
    let mut models: Vec<AgentModel> = Vec::new();
    if let Some(default) = omp_config_default_model() {
        models.push(AgentModel {
            label: format!("{} (config default)", default),
            id: default,
        });
    }
    for entry in parsed.models {
        if entry.selector.trim().is_empty() {
            continue;
        }
        let label = if entry.name.trim().is_empty() {
            entry.selector.clone()
        } else {
            entry.name
        };
        models.push(AgentModel {
            id: entry.selector,
            label,
        });
    }
    if models.is_empty() {
        None
    } else {
        Some(models)
    }
}

/// Read the `default` model role from `~/.omp/agent/config.yml`, if present.
fn omp_config_default_model() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home)
        .join(".omp")
        .join("agent")
        .join("config.yml");
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_model_roles = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("modelRoles:") {
            in_model_roles = true;
            continue;
        }
        if in_model_roles {
            if trimmed.starts_with("symbolPreset:")
                || trimmed.starts_with("theme:")
                || trimmed.starts_with("setupVersion:")
            {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("default:") {
                let value = rest
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"')
                    .to_string();
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
    }
    None
}

pub fn find_real_binary(name: &str) -> Option<PathBuf> {
    if name.trim().is_empty() {
        return None;
    }

    let candidates = if cfg!(target_os = "windows") {
        find_binary_paths_windows(name)
    } else {
        find_binary_paths_unix(name)
    };

    candidates.into_iter().next()
}

fn find_binary_paths_unix(name: &str) -> Vec<PathBuf> {
    // Fast path: direct PATH scan (sub-ms, no shell spawn). This is all the
    // dev app ever needs — the terminal inherits the full shell PATH.
    let from_path = find_binary_paths_in_path(name);
    if !from_path.is_empty() {
        return from_path;
    }

    // Slow fallback: the GUI-launched app (Finder/Dock) inherits launchd's
    // minimal PATH, so binaries in shell-only dirs (~/.local/bin, custom
    // homebrew) are missed above. Source a login shell as the final check.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let quoted = shlex::try_quote(name).unwrap_or_else(|_| std::borrow::Cow::Borrowed(name));
    let delimiter = "__AGENT_IDE_WHICH_DELIMITER__";
    let script = format!(
        "printf '%s' '{}'; which -a -- {}; printf '%s' '{}'",
        delimiter, quoted, delimiter
    );

    let output = match Command::new(&shell).args(["-il", "-c", &script]).output() {
        Ok(out) if out.status.success() => out.stdout,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output);
    let sections = text.split(delimiter).collect::<Vec<_>>();
    let raw = if sections.len() >= 3 { sections[1] } else { "" };

    let paths = parse_which_output(raw.as_bytes());
    let filtered = filter_wrapper_paths(paths);
    filtered
}

fn find_binary_paths_windows(name: &str) -> Vec<PathBuf> {
    let output = match Command::new("where.exe").arg(name).output() {
        Ok(out) if out.status.success() => out.stdout,
        _ => return find_binary_paths_in_path(name),
    };

    let paths = parse_which_output(&output);
    let filtered = filter_wrapper_paths(paths);
    if filtered.is_empty() {
        return find_binary_paths_in_path(name);
    }
    filtered
}

fn find_binary_paths_in_path(name: &str) -> Vec<PathBuf> {
    let path_var = match std::env::var_os("PATH") {
        Some(v) => v,
        None => return Vec::new(),
    };

    let candidates: Vec<PathBuf> = std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .filter(|candidate| is_valid_binary(candidate))
        .collect();
    filter_wrapper_paths(candidates)
}

fn parse_which_output(output: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(output)
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| line.starts_with('/') || (cfg!(windows) && line.contains('\\')))
        .map(PathBuf::from)
        .filter(|p| is_valid_binary(p))
        .collect()
}

fn is_valid_binary(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    if !path.exists() {
        return false;
    }
    if !path.is_file() {
        return false;
    }
    is_executable(path)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => {
            let mode = meta.permissions().mode();
            meta.is_file() && (mode & 0o111) != 0
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn filter_wrapper_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return paths;
    };

    let superset_bin = home.join(".superset").join("bin");
    let superset_prefix = home.join(".superset-");

    paths
        .into_iter()
        .filter(|p| {
            let Ok(normalized) = p.canonicalize() else {
                return true;
            };
            !normalized.starts_with(&superset_bin)
                && !(normalized.starts_with(&superset_prefix) && normalized.components().any(|c| {
                    c.as_os_str() == std::ffi::OsStr::new("bin")
                }))
        })
        .collect()
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_omp_models_json_entries() {
        let json = r#"{"models":[
            {"provider":"openrouter","id":"~x/y","selector":"openrouter/~x/y","name":"X Y"},
            {"provider":"openrouter","id":"~a/b","selector":"openrouter/~a/b","name":"A B"}
        ]}"#;
        let parsed: OmpModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.models.len(), 2);
        assert_eq!(parsed.models[0].selector, "openrouter/~x/y");
        assert_eq!(parsed.models[0].name, "X Y");
    }

    #[test]
    fn omp_models_response_allows_missing_fields() {
        let parsed: OmpModelsResponse = serde_json::from_str(r#"{"models":[]}"#).unwrap();
        assert!(parsed.models.is_empty());
    }

    #[test]
    fn omp_models_skips_empty_selector() {
        // Entries without a usable selector must be skipped, not emitted as
        // blank options.
        let json = r#"{"models":[
            {"selector":"openrouter/~x/y","name":"X Y"},
            {"selector":"","name":"Broken"}
        ]}"#;
        let parsed: OmpModelsResponse = serde_json::from_str(json).unwrap();
        let count = parsed.models.iter().filter(|m| !m.selector.trim().is_empty()).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn resume_flag_covers_agents_with_continue_support() {
        assert_eq!(resume_flag("claude"), Some("-c"));
        assert_eq!(resume_flag("omp"), Some("-c"));
        assert_eq!(resume_flag("opencode"), Some("-c"));
        assert_eq!(resume_flag("codex"), None);
        assert_eq!(resume_flag("unknown"), None);
    }

    #[test]
    fn global_hook_registration_merges_and_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", home.path());
        let marker = home.path().join("shims").join("session-marker.sh");
        std::fs::create_dir_all(home.path().join("shims")).unwrap();

        // First registration creates the settings file.
        ensure_global_claude_hook(&marker).unwrap();
        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let entries = settings["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(entries.len(), 1);

        // Existing user hooks are preserved and duplicates are not added.
        let with_user_hooks = serde_json::json!({
            "env": { "FOO": "1" },
            "hooks": {
                "SessionStart": [
                    { "hooks": [ { "type": "command", "command": "user-own-hook.sh" } ] }
                ]
            }
        });
        std::fs::write(
            home.path().join(".claude/settings.json"),
            serde_json::to_string_pretty(&with_user_hooks).unwrap(),
        )
        .unwrap();
        ensure_global_claude_hook(&marker).unwrap();
        ensure_global_claude_hook(&marker).unwrap();
        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(settings["env"]["FOO"], "1");
        let entries = settings["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        std::env::remove_var("HOME");
    }
}
