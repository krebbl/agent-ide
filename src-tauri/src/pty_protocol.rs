use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub session_id: String,
    pub session_type: String,
    pub cwd: Option<String>,
    pub title: String,
    pub is_busy: bool,
    pub project_id: Option<String>,
    pub worktree_id: Option<String>,
    /// Foreground coding-agent binary name (claude, omp, ...), when detected.
    #[serde(default)]
    pub agent_name: Option<String>,
    /// Live flag: a coding agent is the current foreground process right now.
    /// Unlike `agent_name` (sticky session history), this clears when the
    /// agent finishes.
    #[serde(default)]
    pub agent_active: bool,
    /// Epoch ms when the session was created. Persisted, so it survives daemon
    /// restarts and gives the frontend a stable creation-order sort key.
    #[serde(default)]
    pub created_at: u64,
    /// Local PTY process group id, used to enumerate the processes (shell +
    /// jobs) running in this terminal session. `None` for remote sessions.
    pub pgid: Option<i32>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: i32,
    pub pgid: i32,
    pub comm: String,
    pub args: String,
    /// True when this process row was identified as a coding agent.
    #[serde(default)]
    pub is_agent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "camelCase")]
pub enum DaemonRequest {
    CreateLocal {
        session_id: String,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        project_id: Option<String>,
        worktree_id: Option<String>,
    },
    CreateRemote {
        session_id: String,
        project_id: String,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        worktree_id: Option<String>,
        attach: bool,
    },
    RegisterSshProject {
        project_id: String,
        host: String,
        port: u16,
        username: String,
        auth_method: String,
        key_path: Option<String>,
        password: Option<String>,
    },
    Write {
        session_id: String,
        data: String,
    },
    Resize {
        session_id: String,
        cols: u16,
        rows: u16,
    },
    Kill {
        session_id: String,
    },
    ListSessions,
    AttachAll,
    ProcessList {
        session_id: String,
    },
    Version {
        token: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "camelCase")]
pub enum DaemonEvent {
    Output {
        session_id: String,
        data: String,
    },
    Idle {
        session_id: String,
        title: String,
    },
    Busy {
        session_id: String,
        title: String,
    },
    Exit {
        session_id: String,
        exit_code: Option<i32>,
    },
    Title {
        session_id: String,
        title: String,
    },
    Agent {
        session_id: String,
        name: Option<String>,
    },
    SessionList {
        sessions: Vec<SessionMeta>,
    },
    ProcessList {
        session_id: String,
        processes: Vec<ProcessInfo>,
    },
    StateSnapshot {
        session_id: String,
        is_busy: bool,
        title: String,
    },
    Error {
        session_id: Option<String>,
        message: String,
    },
    Version {
        token: String,
    },
}
