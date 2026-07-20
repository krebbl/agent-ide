use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptTransport {
    Argv,
    Stdin,
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
        },
        AgentDefinition {
            id: "amp".to_string(),
            label: "Amp".to_string(),
            description: "Amp's coding agent for terminal-first coding, subagents, and task work.".to_string(),
            command: vec!["amp".to_string()],
            prompt_transport: PromptTransport::Stdin,
            enabled: true,
            include_in_default_presets: false,
        },
        AgentDefinition {
            id: "codex".to_string(),
            label: "Codex".to_string(),
            description: "OpenAI's coding agent for reading, modifying, and running code across tasks.".to_string(),
            command: vec!["codex".to_string(), "--dangerously-bypass-approvals-and-sandbox".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: true,
        },
        AgentDefinition {
            id: "gemini".to_string(),
            label: "Gemini".to_string(),
            description: "Google's terminal agent for coding, problem-solving, and task work.".to_string(),
            command: vec!["gemini".to_string(), "--approval-mode=auto_edit".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: true,
        },
        AgentDefinition {
            id: "mastracode".to_string(),
            label: "Mastracode".to_string(),
            description: "Mastra's coding agent for building, debugging, and shipping code from the terminal.".to_string(),
            command: vec!["mastracode".to_string()],
            prompt_transport: PromptTransport::Stdin,
            enabled: true,
            include_in_default_presets: false,
        },
        AgentDefinition {
            id: "opencode".to_string(),
            label: "OpenCode".to_string(),
            description: "Open-source AI coding agent with full file and shell access by default.".to_string(),
            command: vec!["opencode".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: false,
        },
        AgentDefinition {
            id: "pi".to_string(),
            label: "Pi".to_string(),
            description: "Minimal terminal coding harness.".to_string(),
            command: vec!["pi".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: false,
        },
        AgentDefinition {
            id: "copilot".to_string(),
            label: "Copilot".to_string(),
            description: "GitHub Copilot agent for terminal-based coding tasks.".to_string(),
            command: vec!["copilot".to_string(), "--allow-tool=write".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: false,
        },
        AgentDefinition {
            id: "cursor-agent".to_string(),
            label: "Cursor Agent".to_string(),
            description: "Cursor's coding agent that prompts for every action.".to_string(),
            command: vec!["cursor-agent".to_string()],
            prompt_transport: PromptTransport::Argv,
            enabled: true,
            include_in_default_presets: false,
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
    builtin_agents()
        .into_iter()
        .map(|agent| {
            let binary_path = agent
                .command
                .first()
                .and_then(|name| find_real_binary(name));
            agent_to_status(agent, binary_path)
        })
        .collect()
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
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let quoted = shlex::try_quote(name).unwrap_or_else(|_| std::borrow::Cow::Borrowed(name));
    let delimiter = "__AGENT_IDE_WHICH_DELIMITER__";
    let script = format!(
        "printf '%s' '{}'; which -a -- {}; printf '%s' '{}'",
        delimiter, quoted, delimiter
    );

    let output = match Command::new(&shell).args(["-il", "-c", &script]).output() {
        Ok(out) if out.status.success() => out.stdout,
        _ => return find_binary_paths_in_path(name),
    };

    let text = String::from_utf8_lossy(&output);
    let sections = text.split(delimiter).collect::<Vec<_>>();
    let raw = if sections.len() >= 3 { sections[1] } else { "" };

    let paths = parse_which_output(raw.as_bytes());
    let filtered = filter_wrapper_paths(paths);
    if filtered.is_empty() {
        return find_binary_paths_in_path(name);
    }
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
