use agent_ide_lib::{commands, AppState};
use serde_json::Value;

pub async fn dispatch(
    state: &AppState,
    command: &str,
    body: Value,
) -> Result<Value, String> {
    commands::dispatch(state, command, body).await
}
