//! Cross-restart history of agent conversations seen in local terminals.
//!
//! The pty daemon records every live conversation (id, agent, cwd) it learns
//! from the SessionStart marker hooks into a small JSON file next to the
//! terminal-session persistence file. The frontend reads it to offer
//! "resume last session" for a worktree even after that terminal is closed —
//! markers alone are keyed by ephemeral pty ids and vanish with them.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Cap on stored conversations; oldest entries are evicted.
const MAX_ENTRIES: usize = 500;
pub const FILE_NAME: &str = "agent_conversations.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationEntry {
    pub agent: Option<String>,
    pub conversation_id: String,
    pub cwd: String,
    pub first_seen_at: u64,
    pub last_active_at: u64,
}

/// History file location for a daemon persistence path (sibling file).
pub fn path_for(persistence_path: &Path) -> PathBuf {
    persistence_path
        .parent()
        .map(|p| p.join(FILE_NAME))
        .unwrap_or_else(|| persistence_path.with_extension("history"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn load(path: &Path) -> Vec<ConversationEntry> {
    std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

fn store(path: &Path, entries: &[ConversationEntry]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(entries) {
        let _ = std::fs::write(path, json);
    }
}

/// Batch-upsert live conversations. Bumps `last_active_at` for the given
/// entries, fills a missing `agent`, and records new conversations. Only
/// rewrites the file when something actually changed, so idle terminals
/// never touch disk.
pub fn upsert_all(path: &Path, updates: &[Upsert]) {
    if updates.is_empty() {
        return;
    }
    let mut entries = load(path);
    let now = now_ms();
    let mut changed = false;
    for update in updates {
        let bump = update.bump_active.then_some(now);
        match entries
            .iter_mut()
            .find(|e| e.conversation_id == update.conversation_id)
        {
            Some(entry) => {
                if entry.cwd != update.cwd {
                    entry.cwd = update.cwd.clone();
                    changed = true;
                }
                if entry.agent.is_none() {
                    if let Some(agent) = update.agent.clone() {
                        entry.agent = Some(agent);
                        changed = true;
                    }
                }
                if let Some(t) = bump {
                    entry.last_active_at = t;
                    changed = true;
                }
            }
            None => {
                // A first sighting is activity even without an explicit
                // bump (restored sessions carry their conversation from the
                // previous run and never see a marker change).
                entries.push(ConversationEntry {
                    agent: update.agent.clone(),
                    conversation_id: update.conversation_id.clone(),
                    cwd: update.cwd.clone(),
                    first_seen_at: now,
                    last_active_at: now,
                });
                changed = true;
            }
        }
    }
    if !changed {
        return;
    }
    entries.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    entries.truncate(MAX_ENTRIES);
    store(path, &entries);
}

/// One live-conversation observation from the daemon.
pub struct Upsert {
    pub cwd: String,
    pub agent: Option<String>,
    pub conversation_id: String,
    /// Bump `last_active_at` to now: set for conversations whose marker just
    /// changed, not for every re-observation.
    pub bump_active: bool,
}

/// Conversations recorded for `cwd`, most recently active first.
pub fn for_cwd(path: &Path, cwd: &str) -> Vec<ConversationEntry> {
    let mut entries: Vec<ConversationEntry> = load(path)
        .into_iter()
        .filter(|e| e.cwd == cwd)
        .collect();
    entries.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_creates_and_updates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);

        upsert_all(
            &path,
            &[Upsert {
                cwd: "/repo".to_string(),
                agent: None,
                conversation_id: "c1".to_string(),
                bump_active: true,
            }],
        );
        let entries = for_cwd(&path, "/repo");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].agent, None);
        assert_eq!(entries[0].conversation_id, "c1");

        // Later observation with a detected agent fills it in without
        // losing the entry.
        upsert_all(
            &path,
            &[Upsert {
                cwd: "/repo".to_string(),
                agent: Some("claude".to_string()),
                conversation_id: "c1".to_string(),
                bump_active: false,
            }],
        );
        let entries = for_cwd(&path, "/repo");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].agent.as_deref(), Some("claude"));
    }

    #[test]
    fn upsert_all_is_noop_without_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        upsert_all(
            &path,
            &[Upsert {
                cwd: "/repo".to_string(),
                agent: Some("claude".to_string()),
                conversation_id: "c1".to_string(),
                bump_active: true,
            }],
        );
        let before = std::fs::read(&path).unwrap();

        // Same data, no bump: file must not be rewritten.
        upsert_all(
            &path,
            &[Upsert {
                cwd: "/repo".to_string(),
                agent: Some("claude".to_string()),
                conversation_id: "c1".to_string(),
                bump_active: false,
            }],
        );
        assert_eq!(before, std::fs::read(&path).unwrap());
    }

    #[test]
    fn for_cwd_filters_and_sorts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        upsert_all(
            &path,
            &[
                Upsert {
                    cwd: "/a".to_string(),
                    agent: Some("claude".to_string()),
                    conversation_id: "c1".to_string(),
                    bump_active: true,
                },
                Upsert {
                    cwd: "/b".to_string(),
                    agent: Some("omp".to_string()),
                    conversation_id: "c2".to_string(),
                    bump_active: true,
                },
            ],
        );
        let a = for_cwd(&path, "/a");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].conversation_id, "c1");
        assert!(for_cwd(&path, "/missing").is_empty());
    }

    #[test]
    fn first_sighting_without_bump_still_timestamps() {
        // Restored sessions carry their conversation from the previous run
        // and never see a marker change; they must not record time 0.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        upsert_all(
            &path,
            &[Upsert {
                cwd: "/repo".to_string(),
                agent: Some("claude".to_string()),
                conversation_id: "c9".to_string(),
                bump_active: false,
            }],
        );
        let entries = for_cwd(&path, "/repo");
        assert!(entries[0].last_active_at > 0);
        assert!(entries[0].first_seen_at > 0);
    }

    #[test]
    fn entries_are_capped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        for i in 0..600 {
            upsert_all(
                &path,
                &[Upsert {
                    cwd: "/repo".to_string(),
                    agent: Some("claude".to_string()),
                    conversation_id: format!("c{}", i),
                    bump_active: true,
                }],
            );
        }
        assert_eq!(for_cwd(&path, "/repo").len(), MAX_ENTRIES);
        // Newest survived.
        assert!(
            for_cwd(&path, "/repo")
                .iter()
                .any(|e| e.conversation_id == "c599")
        );
    }
}
