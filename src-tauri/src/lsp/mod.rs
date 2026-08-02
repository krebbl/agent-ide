pub mod process;
pub mod registry;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::{AppState, Connection};

const REQUEST_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspStatusEvent {
    pub project_id: String,
    pub language_id: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LspMessageEvent {
    pub project_id: String,
    pub language_id: String,
    pub message: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct LspServerInfo {
    pub project_id: String,
    pub language_id: String,
    pub status: String,
    pub capabilities: Value,
    pub server_info: Value,
}

struct LspTransport {
    outgoing: mpsc::Sender<Value>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    next_id: AtomicU64,
}

impl LspTransport {
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.outgoing
            .send(msg)
            .await
            .map_err(|_| "Language server writer closed".to_string())?;
        match tokio::time::timeout(
            std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
            rx,
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("Language server dropped the request".to_string()),
            Err(_) => Err(format!("LSP request '{}' timed out", method)),
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.outgoing
            .send(msg)
            .await
            .map_err(|_| "Language server writer closed".to_string())
    }

    async fn fail_all_pending(&self, reason: &str) {
        let mut pending = self.pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err(reason.to_string()));
        }
    }
}

pub struct LspClient {
    project_id: String,
    language_id: String,
    transport: Arc<LspTransport>,
    child: Mutex<Child>,
    stopping: Arc<AtomicBool>,
    capabilities: Value,
    server_info: Value,
}

impl LspClient {
    fn info(&self) -> LspServerInfo {
        LspServerInfo {
            project_id: self.project_id.clone(),
            language_id: self.language_id.clone(),
            status: "ready".to_string(),
            capabilities: self.capabilities.clone(),
            server_info: self.server_info.clone(),
        }
    }

    async fn shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _ = self.transport.request("shutdown", Value::Null).await;
        let _ = self.transport.notify("exit", Value::Null).await;
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
    }
}

#[derive(Default)]
pub struct LspManager {
    clients: Mutex<HashMap<String, Arc<LspClient>>>,
}

fn client_key(project_id: &str, language_id: &str) -> String {
    format!("{}:{}", project_id, language_id)
}

impl LspManager {
    pub async fn get(&self, project_id: &str, language_id: &str) -> Option<Arc<LspClient>> {
        self.clients
            .lock()
            .await
            .get(&client_key(project_id, language_id))
            .cloned()
    }

    async fn insert(&self, client: Arc<LspClient>) {
        self.clients.lock().await.insert(
            client_key(&client.project_id, &client.language_id),
            client,
        );
    }

    async fn remove(&self, project_id: &str, language_id: &str) -> Option<Arc<LspClient>> {
        self.clients
            .lock()
            .await
            .remove(&client_key(project_id, language_id))
    }

    async fn list(&self) -> Vec<Arc<LspClient>> {
        self.clients.lock().await.values().cloned().collect()
    }
}

fn emit_status(
    app_handle: Option<&tauri::AppHandle>,
    project_id: &str,
    language_id: &str,
    status: &str,
    error: Option<String>,
) {
    if let Some(handle) = app_handle {
        let _ = handle.emit(
            "lsp://status",
            LspStatusEvent {
                project_id: project_id.to_string(),
                language_id: language_id.to_string(),
                status: status.to_string(),
                error,
            },
        );
    }
}

fn emit_message(
    app_handle: &Option<tauri::AppHandle>,
    project_id: &str,
    language_id: &str,
    message: Value,
) {
    if let Some(handle) = app_handle {
        let _ = handle.emit(
            "lsp://message",
            LspMessageEvent {
                project_id: project_id.to_string(),
                language_id: language_id.to_string(),
                message,
            },
        );
    }
}

fn initialize_params(root_uri: &str) -> Value {
    json!({
        "processId": std::process::id(),
        "rootUri": root_uri,
        "clientInfo": {
            "name": "agent-ide",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "capabilities": {
            "textDocument": {
                "synchronization": {
                    "dynamicRegistration": false,
                    "willSave": false,
                    "didSave": true
                },
                "completion": {
                    "completionItem": {
                        "snippetSupport": false,
                        "documentationFormat": ["markdown", "plaintext"]
                    }
                },
                "hover": {
                    "contentFormat": ["markdown", "plaintext"]
                },
                "definition": {},
                "references": {},
                "rename": {},
                "documentSymbol": {
                    "hierarchicalDocumentSymbolSupport": true
                },
                "publishDiagnostics": {}
            },
            "workspace": {
                "applyEdit": true,
                "symbol": {},
                "workspaceFolders": { "supported": false }
            }
        },
        "workspaceFolders": null
    })
}

pub async fn start_server(
    app_handle: Option<tauri::AppHandle>,
    manager: &LspManager,
    project_id: &str,
    language_id: &str,
    root_path: &str,
) -> Result<LspServerInfo, String> {
    if let Some(existing) = manager.get(project_id, language_id).await {
        if !existing.transport.outgoing.is_closed() {
            return Ok(existing.info());
        }
        manager.remove(project_id, language_id).await;
    }

    emit_status(app_handle.as_ref(), project_id, language_id, "starting", None);

    let result = spawn_and_initialize(
        app_handle.clone(),
        project_id,
        language_id,
        root_path,
    )
    .await;

    match result {
        Ok(client) => {
            let info = client.info();
            manager.insert(client).await;
            emit_status(app_handle.as_ref(), project_id, language_id, "ready", None);
            Ok(info)
        }
        Err(e) => {
            emit_status(
                app_handle.as_ref(),
                project_id,
                language_id,
                "crashed",
                Some(e.clone()),
            );
            Err(e)
        }
    }
}

async fn spawn_and_initialize(
    app_handle: Option<tauri::AppHandle>,
    project_id: &str,
    language_id: &str,
    root_path: &str,
) -> Result<Arc<LspClient>, String> {
    let spec = registry::server_for_language(language_id)
        .ok_or_else(|| format!("No language server registered for '{}'", language_id))?;
    let program = registry::resolve_on_path(spec.command)
        .ok_or_else(|| format!("Language server '{}' not found on PATH", spec.command))?;

    let spawned = process::spawn_local(&program, spec.args, Some(root_path))?;
    let process::SpawnedServer {
        child,
        mut stdin,
        mut stdout,
    } = spawned;

    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<Value>(64);
    let transport = Arc::new(LspTransport {
        outgoing: outgoing_tx,
        pending: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
    });
    let stopping = Arc::new(AtomicBool::new(false));

    tokio::spawn(async move {
        while let Some(msg) = outgoing_rx.recv().await {
            if process::write_message(&mut stdin, &msg).await.is_err() {
                break;
            }
        }
    });

    {
        let transport = Arc::clone(&transport);
        let stopping = Arc::clone(&stopping);
        let project_id = project_id.to_string();
        let language_id = language_id.to_string();
        tokio::spawn(async move {
            loop {
                match process::read_message(&mut stdout).await {
                    Ok(Some(msg)) => {
                        let has_id = msg.get("id").is_some();
                        let is_response = has_id
                            && (msg.get("result").is_some() || msg.get("error").is_some());
                        if is_response {
                            if let Some(id) = msg.get("id").and_then(|v| v.as_u64()) {
                                if let Some(tx) = transport.pending.lock().await.remove(&id) {
                                    if let Some(err) = msg.get("error") {
                                        let _ = tx.send(Err(err.to_string()));
                                    } else {
                                        let _ = tx.send(Ok(msg
                                            .get("result")
                                            .cloned()
                                            .unwrap_or(Value::Null)));
                                    }
                                }
                            }
                        } else if has_id && msg.get("method").is_some() {
                            if let Some(id) = msg.get("id").cloned() {
                                let _ = transport
                                    .outgoing
                                    .send(json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": Value::Null
                                    }))
                                    .await;
                            }
                            emit_message(&app_handle, &project_id, &language_id, msg);
                        } else {
                            emit_message(&app_handle, &project_id, &language_id, msg);
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!("LSP reader error for {}: {}", language_id, e);
                        break;
                    }
                }
            }
            transport.fail_all_pending("Language server exited").await;
            let status = if stopping.load(Ordering::SeqCst) {
                "stopped"
            } else {
                "crashed"
            };
            emit_status(
                app_handle.as_ref(),
                &project_id,
                &language_id,
                status,
                None,
            );
        });
    }

    let root_uri = registry::path_to_uri(root_path);
    let init_result = transport
        .request("initialize", initialize_params(&root_uri))
        .await;
    let result = match init_result {
        Ok(r) => r,
        Err(e) => {
            let mut child = child;
            let _ = child.kill().await;
            return Err(format!("LSP initialize failed: {}", e));
        }
    };
    transport.notify("initialized", json!({})).await?;

    Ok(Arc::new(LspClient {
        project_id: project_id.to_string(),
        language_id: language_id.to_string(),
        transport,
        child: Mutex::new(child),
        stopping,
        capabilities: result.get("capabilities").cloned().unwrap_or(Value::Null),
        server_info: result.get("serverInfo").cloned().unwrap_or(Value::Null),
    }))
}

#[tauri::command]
pub async fn lsp_start(
    project_id: String,
    language_id: String,
    root_path: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<LspServerInfo, String> {
    let projects = crate::load_projects(app.clone())?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;
    if matches!(project.connection, Connection::Ssh { .. }) {
        return Err("Remote language servers over SSH are not supported yet".to_string());
    }
    start_server(
        Some(app),
        &state.lsp_manager,
        &project_id,
        &language_id,
        &root_path,
    )
    .await
}

#[tauri::command]
pub async fn lsp_request(
    project_id: String,
    language_id: String,
    method: String,
    params: Value,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Value, String> {
    let client = state
        .lsp_manager
        .get(&project_id, &language_id)
        .await
        .ok_or("Language server is not running")?;
    client.transport.request(&method, params).await
}

#[tauri::command]
pub async fn lsp_notify(
    project_id: String,
    language_id: String,
    method: String,
    params: Value,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let client = state
        .lsp_manager
        .get(&project_id, &language_id)
        .await
        .ok_or("Language server is not running")?;
    client.transport.notify(&method, params).await
}

#[tauri::command]
pub async fn lsp_stop(
    project_id: String,
    language_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if let Some(client) = state.lsp_manager.remove(&project_id, &language_id).await {
        client.shutdown().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn lsp_list(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<LspServerInfo>, String> {
    Ok(state
        .lsp_manager
        .list()
        .await
        .iter()
        .map(|c| c.info())
        .collect())
}

#[tauri::command]
pub fn lsp_server_available(language_id: String) -> bool {
    registry::server_for_language(&language_id)
        .and_then(|spec| registry::resolve_on_path(spec.command))
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn smoke_typescript_language_server() {
        let manager = LspManager::default();
        let cwd = std::env::current_dir().unwrap();
        let root = cwd.parent().unwrap().to_string_lossy().to_string();
        let info = start_server(None, &manager, "test-project", "typescript", &root)
            .await
            .unwrap();
        assert!(info.capabilities.is_object());
        let client = manager.get("test-project", "typescript").await.unwrap();
        client
            .transport
            .notify(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": "file:///tmp/agent-ide-lsp-smoke.ts",
                        "languageId": "typescript",
                        "version": 1,
                        "text": "const x: number = 1;\nconsole.log(x);\n"
                    }
                }),
            )
            .await
            .unwrap();
        let hover = client
            .transport
            .request(
                "textDocument/hover",
                json!({
                    "textDocument": { "uri": "file:///tmp/agent-ide-lsp-smoke.ts" },
                    "position": { "line": 0, "character": 6 }
                }),
            )
            .await;
        assert!(hover.is_ok(), "hover request failed: {:?}", hover.err());
        client.shutdown().await;
        manager.remove("test-project", "typescript").await;
    }
}
