use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Clone)]
pub enum EventBus {
    Tauri(tauri::AppHandle),
    Broadcast(broadcast::Sender<ServerEvent>),
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEvent {
    pub event: String,
    pub payload: serde_json::Value,
}

impl EventBus {
    pub fn emit(&self, event_name: &str, payload: impl Serialize) {
        let payload_value = match serde_json::to_value(payload) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(event = event_name, error = %e, "failed to serialize event payload");
                return;
            }
        };

        match self {
            EventBus::Tauri(handle) => {
                let _ = tauri::Emitter::emit(handle, event_name, payload_value);
            }
            EventBus::Broadcast(tx) => {
                let event = ServerEvent {
                    event: event_name.to_string(),
                    payload: payload_value,
                };
                let _ = tx.send(event);
            }
        }
    }
}
