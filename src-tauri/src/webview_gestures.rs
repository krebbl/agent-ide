#[cfg(target_os = "macos")]
#[link(name = "agent_ide_notification", kind = "static")]
extern "C" {
    fn enable_webview_back_forward_gestures(ns_view: *mut std::ffi::c_void);
}

/// Enables WKWebView back/forward navigation gestures (two-finger swipe)
/// on the window's webview. No-op on other platforms.
pub fn enable(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        if let Ok(view) = window.ns_view() {
            unsafe {
                enable_webview_back_forward_gestures(view);
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
    }
}
