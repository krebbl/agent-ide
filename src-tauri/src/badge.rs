use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::AppHandle;

/// Count of finished agent runs + finished terminal sessions, surfaced as a
/// macOS dock badge (other platforms: counted, but no dock to draw on).
///
/// The count lives in Rust and is persisted to the config dir so it survives
/// app relaunch, per product decision. It resets to zero when the main window
/// gains focus (macOS convention: focus = seen).
pub struct DockBadge {
    count: AtomicU64,
    handle: AppHandle,
}

fn badge_path() -> Result<PathBuf, String> {
    Ok(crate::config::app_config_dir()?.join("badge_count.json"))
}

fn read_persisted() -> u64 {
    let path = match badge_path() {
        Ok(p) => p,
        Err(_) => return 0,
    };
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

fn persist(count: u64) {
    if let Ok(path) = badge_path() {
        let _ = fs::write(path, count.to_string());
    }
}

impl DockBadge {
    /// Loads the persisted count and applies it to the dock immediately, so a
    /// relaunch restores the badge before any new event arrives.
    pub fn new(handle: AppHandle) -> Self {
        let count = read_persisted();
        let badge = Self {
            count: AtomicU64::new(count),
            handle,
        };
        crate::mac_badge::apply(&badge.handle, count);
        badge
    }

    /// +1, persist, and re-render the badge. Called from the pty-read task for
    /// every finished agent run (`Agent { name: None }`) and every finished
    /// terminal session (`Exit`).
    pub fn increment(&self) {
        let n = self.count.fetch_add(1, Ordering::SeqCst) + 1;
        persist(n);
        tracing::debug!(badge_count = n, "badge increment");
        crate::mac_badge::apply(&self.handle, n);
    }

    /// Reset to zero and drop the badge. Called when the main window gains
    /// focus. Persisted, so a stale count is never resurrected on relaunch.
    pub fn clear(&self) {
        self.count.store(0, Ordering::SeqCst);
        persist(0);
        crate::mac_badge::apply(&self.handle, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_persists_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("AGENT_IDE_CONFIG_DIR", dir.path());

        // First "launch": increment logic mirrors write+reload via `persist`.
        persist(3);
        assert_eq!(read_persisted(), 3);

        // Second "launch": what `DockBadge::new` restores.
        assert_eq!(read_persisted(), 3);

        // Clear persists zero, never resurrected on next relaunch.
        persist(0);
        assert_eq!(read_persisted(), 0);
    }
}