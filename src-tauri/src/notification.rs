#[cfg(target_os = "macos")]
use std::ffi::CString;

#[cfg(target_os = "macos")]
#[link(name = "agent_ide_notification", kind = "static")]
extern "C" {
    fn show_notification(title: *const i8, body: *const i8);
}

pub fn show(title: &str, body: &str) -> Result<(), String> {
    tracing::info!(title, body, "showing native notification");
    #[cfg(target_os = "macos")]
    {
        let title_c = CString::new(title).map_err(|e| e.to_string())?;
        let body_c = CString::new(body).map_err(|e| e.to_string())?;
        unsafe {
            show_notification(title_c.as_ptr() as *const i8, body_c.as_ptr() as *const i8);
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = title;
        let _ = body;
        Ok(())
    }
}

#[tauri::command]
pub async fn notification_show(title: String, body: String) -> Result<(), String> {
    tracing::info!(title, body, "notification_show command called");
    show(&title, &body)
}
