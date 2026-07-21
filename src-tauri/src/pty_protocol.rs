use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub session_id: String,
    pub session_type: String,
    pub cwd: Option<String>,
    pub title: String,
    pub is_busy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "camelCase")]
pub enum DaemonRequest {
    CreateLocal {
        session_id: String,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
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
    SessionList {
        sessions: Vec<SessionMeta>,
    },
    StateSnapshot {
        session_id: String,
        is_busy: bool,
        title: String,
    },
    Error {
        message: String,
    },
}
