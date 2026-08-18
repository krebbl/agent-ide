#[cfg(target_os = "macos")]
use std::ffi::{CStr, CString};
use std::sync::OnceLock;

#[cfg(target_os = "macos")]
use serde::Serialize;

use crate::event_bus::EventBus;

static EVENT_BUS: OnceLock<EventBus> = OnceLock::new();

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationClickedEvent {
    pub session_id: String,
}

#[cfg(target_os = "macos")]
extern "C" fn on_notification_clicked(session_id: *const i8) {
    if session_id.is_null() {
        return;
    }
    let id = unsafe { CStr::from_ptr(session_id) }
        .to_string_lossy()
        .to_string();
    tracing::info!(session_id = %id, "notification clicked");
    if let Some(event_bus) = EVENT_BUS.get() {
        event_bus.emit(
            "notification_clicked",
            NotificationClickedEvent { session_id: id },
        );
    }
}

pub fn set_event_bus(event_bus: EventBus) {
    let _ = EVENT_BUS.set(event_bus);
    #[cfg(target_os = "macos")]
    unsafe {
        set_notification_clicked_callback(on_notification_clicked);
    }
}

#[cfg(target_os = "macos")]
#[link(name = "agent_ide_notification", kind = "static")]
extern "C" {
    fn show_notification(title: *const i8, body: *const i8, session_id: *const i8);
    fn set_notification_clicked_callback(callback: extern "C" fn(*const i8));
}

pub fn show(title: &str, body: &str, session_id: Option<&str>) -> Result<(), String> {
    tracing::info!(title, body, ?session_id, "showing native notification");
    #[cfg(target_os = "macos")]
    {
        let title_c = CString::new(title).map_err(|e| e.to_string())?;
        let body_c = CString::new(body).map_err(|e| e.to_string())?;
        let session_id_c = session_id.map(|s| CString::new(s).map_err(|e| e.to_string())).transpose()?;
        unsafe {
            show_notification(
                title_c.as_ptr() as *const i8,
                body_c.as_ptr() as *const i8,
                session_id_c.as_ref().map_or(std::ptr::null(), |s| s.as_ptr() as *const i8),
            );
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = title;
        let _ = body;
        let _ = session_id;
        Ok(())
    }
}

pub async fn cmd_notification_show(
    title: String,
    body: String,
    session_id: Option<String>,
) -> Result<(), String> {
    tracing::info!(title, body, ?session_id, "notification_show command called");
    show(&title, &body, session_id.as_deref())
}

#[tauri::command]
pub async fn notification_show(
    title: String,
    body: String,
    session_id: Option<String>,
) -> Result<(), String> {
    crate::commands::notification_show(title, body, session_id).await
}
