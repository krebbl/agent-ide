/// macOS dock badge application via AppKit. Tauri has no badge API, so this is
/// a targeted `NSDockTile.setBadgeLabel` call. Non-macOS builds get a no-op
/// (the counter still works in `badge.rs`; there is just no dock to draw on).

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::{NSApplication, NSDockTile};
    use objc2_foundation::{MainThreadMarker, NSString};

    /// MUST run on the main thread (guaranteed by `apply` via
    /// `run_on_main_thread`). `count == 0` clears the badge (`None` label).
    pub fn set_badge(count: u64) {
        let mtm = match MainThreadMarker::new() {
            Some(m) => m,
            None => unsafe { MainThreadMarker::new_unchecked() },
        };
        let app = NSApplication::sharedApplication(mtm);
        let dock_tile: &NSDockTile = &app.dockTile();
        if count == 0 {
            dock_tile.setBadgeLabel(None);
        } else {
            let label = NSString::from_str(&count.to_string());
            dock_tile.setBadgeLabel(Some(&label));
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn set_badge(_count: u64) {}
}

/// Applies `count` to the dock badge. `0` clears it. Safe to call from any
/// thread; the AppKit call is dispatched to the main thread.
pub fn apply(handle: &tauri::AppHandle, count: u64) {
    #[cfg(target_os = "macos")]
    {
        let _ = handle.run_on_main_thread(move || {
            imp::set_badge(count);
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (handle, count);
    }
}

#[cfg(test)]
mod probe_test {
    #[cfg(target_os = "macos")]
    #[test]
    fn dock_badge_sets_and_reads_back() {
        // Probe the exact native path used at runtime: set a label on the
        // app's NSDockTile, then read it back through AppKit itself.
        use objc2_foundation::MainThreadMarker;
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        let tile = app.dockTile();

        crate::mac_badge::imp::set_badge(4);
        let after = tile.badgeLabel().map(|s| s.to_string());
        assert_eq!(after.as_deref(), Some("4"), "setBadgeLabel(4) must be readable back");

        crate::mac_badge::imp::set_badge(0);
        assert!(tile.badgeLabel().is_none(), "clear must remove the badge");
    }
}