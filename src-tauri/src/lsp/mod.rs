pub mod process;
pub mod registry;
pub mod remote;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::event_bus::EventBus;
use crate::remote_ssh::SessionHandle;
use crate::AppState;

const REQUEST_TIMEOUT_SECS: u64 = 60;
const RESTART_DELAYS_SECS: [u64; 3] = [2, 5, 15];

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

#[derive(Clone)]
pub enum SpawnTarget {
    Local,
    Remote(SessionHandle),
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
        match tokio::time::timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS), rx).await {
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

enum ServerHandle {
    Local(Mutex<Child>),
    Remote(Mutex<Option<oneshot::Sender<()>>>),
}

impl ServerHandle {
    async fn kill(&self) {
        match self {
            ServerHandle::Local(child) => {
                let mut child = child.lock().await;
                let _ = child.kill().await;
            }
            ServerHandle::Remote(shutdown) => {
                if let Some(tx) = shutdown.lock().await.take() {
                    let _ = tx.send(());
                }
            }
        }
    }
}

pub struct LspClient {
    project_id: String,
    language_id: String,
    transport: Arc<LspTransport>,
    handle: ServerHandle,
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
        self.handle.kill().await;
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

    pub async fn stop_project(&self, project_id: &str) {
        let prefix = format!("{}:", project_id);
        let clients: Vec<Arc<LspClient>> = {
            let map = self.clients.lock().await;
            map.iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(_, client)| client.clone())
                .collect()
        };
        for client in clients {
            self.remove(&client.project_id, &client.language_id).await;
            client.shutdown().await;
        }
    }

    pub async fn shutdown_all(&self) {
        for client in self.list().await {
            self.remove(&client.project_id, &client.language_id).await;
            client.shutdown().await;
        }
    }
}

pub struct ReaderContext {
    transport: Arc<LspTransport>,
    stopping: Arc<AtomicBool>,
    event_bus: Option<EventBus>,
    project_id: String,
    language_id: String,
    root_path: String,
    target: SpawnTarget,
    manager: Arc<LspManager>,
}

fn emit_status(
    event_bus: Option<&EventBus>,
    project_id: &str,
    language_id: &str,
    status: &str,
    error: Option<String>,
) {
    if let Some(event_bus) = event_bus {
        event_bus.emit(
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
    event_bus: Option<&EventBus>,
    project_id: &str,
    language_id: &str,
    message: Value,
) {
    if let Some(event_bus) = event_bus {
        event_bus.emit(
            "lsp://message",
            LspMessageEvent {
                project_id: project_id.to_string(),
                language_id: language_id.to_string(),
                message,
            },
        );
    }
}

pub(super) async fn dispatch_message(ctx: &ReaderContext, msg: Value) {
    let has_id = msg.get("id").is_some();
    let is_response =
        has_id && (msg.get("result").is_some() || msg.get("error").is_some());
    if is_response {
        if let Some(id) = msg.get("id").and_then(|v| v.as_u64()) {
            if let Some(tx) = ctx.transport.pending.lock().await.remove(&id) {
                if let Some(err) = msg.get("error") {
                    let _ = tx.send(Err(err.to_string()));
                } else {
                    let _ = tx.send(Ok(msg.get("result").cloned().unwrap_or(Value::Null)));
                }
            }
        }
    } else if has_id && msg.get("method").is_some() {
        if let Some(id) = msg.get("id").cloned() {
            let outgoing = ctx.transport.outgoing.clone();
            tokio::spawn(async move {
                let _ = outgoing
                    .send(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": Value::Null
                    }))
                    .await;
            });
        }
        emit_message(ctx.event_bus.as_ref(), &ctx.project_id, &ctx.language_id, msg);
    } else {
        emit_message(ctx.event_bus.as_ref(), &ctx.project_id, &ctx.language_id, msg);
    }
}

pub(super) async fn handle_reader_exit(ctx: ReaderContext) {
    ctx.transport.fail_all_pending("Language server exited").await;
    if ctx.stopping.load(Ordering::SeqCst) {
        emit_status(
            ctx.event_bus.as_ref(),
            &ctx.project_id,
            &ctx.language_id,
            "stopped",
            None,
        );
        return;
    }
    emit_status(
        ctx.event_bus.as_ref(),
        &ctx.project_id,
        &ctx.language_id,
        "crashed",
        None,
    );
    tokio::spawn(auto_restart(ctx));
}

fn auto_restart(ctx: ReaderContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    Box::pin(auto_restart_inner(ctx))
}

async fn auto_restart_inner(ctx: ReaderContext) {
    for delay in RESTART_DELAYS_SECS {
        tokio::time::sleep(Duration::from_secs(delay)).await;
        let still_dead = match ctx.manager.get(&ctx.project_id, &ctx.language_id).await {
            Some(client) => client.transport.outgoing.is_closed(),
            None => false,
        };
        if !still_dead {
            return;
        }
        if start_server(
            ctx.event_bus.clone(),
            ctx.manager.clone(),
            &ctx.project_id,
            &ctx.language_id,
            &ctx.root_path,
            ctx.target.clone(),
        )
        .await
        .is_ok()
        {
            return;
        }
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
                "signatureHelp": {},
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
    event_bus: Option<EventBus>,
    manager: Arc<LspManager>,
    project_id: &str,
    language_id: &str,
    root_path: &str,
    target: SpawnTarget,
) -> Result<LspServerInfo, String> {
    if let Some(existing) = manager.get(project_id, language_id).await {
        if !existing.transport.outgoing.is_closed() {
            return Ok(existing.info());
        }
        manager.remove(project_id, language_id).await;
    }

    emit_status(event_bus.as_ref(), project_id, language_id, "starting", None);

    let result = spawn_and_initialize(
        event_bus.clone(),
        manager.clone(),
        project_id,
        language_id,
        root_path,
        target,
    )
    .await;

    match result {
        Ok(client) => {
            let info = client.info();
            manager.insert(client).await;
            emit_status(event_bus.as_ref(), project_id, language_id, "ready", None);
            Ok(info)
        }
        Err(e) => {
            emit_status(
                event_bus.as_ref(),
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
    event_bus: Option<EventBus>,
    manager: Arc<LspManager>,
    project_id: &str,
    language_id: &str,
    root_path: &str,
    target: SpawnTarget,
) -> Result<Arc<LspClient>, String> {
    let spec = registry::server_for_language(language_id)
        .ok_or_else(|| format!("No language server registered for '{}'", language_id))?;

    let (outgoing_tx, outgoing_rx) = mpsc::channel::<Value>(64);
    let transport = Arc::new(LspTransport {
        outgoing: outgoing_tx,
        pending: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
    });
    let stopping = Arc::new(AtomicBool::new(false));

    let ctx = ReaderContext {
        transport: Arc::clone(&transport),
        stopping: Arc::clone(&stopping),
        event_bus,
        project_id: project_id.to_string(),
        language_id: language_id.to_string(),
        root_path: root_path.to_string(),
        target: target.clone(),
        manager,
    };

    let handle = match &target {
        SpawnTarget::Local => {
            let program = registry::resolve_on_path(spec.command).ok_or_else(|| {
                format!("Language server '{}' not found on PATH", spec.command)
            })?;
            let spawned = process::spawn_local(&program, spec.args, Some(root_path))?;
            let process::SpawnedServer {
                child,
                mut stdin,
                mut stdout,
            } = spawned;

            let mut outgoing_rx = outgoing_rx;
            tokio::spawn(async move {
                while let Some(msg) = outgoing_rx.recv().await {
                    if process::write_message(&mut stdin, &msg).await.is_err() {
                        break;
                    }
                }
            });

            let reader_ctx = ReaderContext { ..ctx };
            tokio::spawn(async move {
                loop {
                    match process::read_message(&mut stdout).await {
                        Ok(Some(msg)) => dispatch_message(&reader_ctx, msg).await,
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(
                                "LSP reader error for {}: {}",
                                reader_ctx.language_id,
                                e
                            );
                            break;
                        }
                    }
                }
                handle_reader_exit(reader_ctx).await;
            });

            ServerHandle::Local(Mutex::new(child))
        }
        SpawnTarget::Remote(session) => {
            let shell = remote::remote_resolve_shell(session, spec.command)
                .await
                .ok_or_else(|| {
                    format!(
                        "Language server '{}' not found on the remote host's PATH",
                        spec.command
                    )
                })?;
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            remote::spawn_remote(
                session.clone(),
                shell,
                &spec,
                root_path,
                outgoing_rx,
                ctx,
                shutdown_rx,
            )
            .await?;
            ServerHandle::Remote(Mutex::new(Some(shutdown_tx)))
        }
    };

    let root_uri = registry::path_to_uri(root_path);
    let init_result = transport
        .request("initialize", initialize_params(&root_uri))
        .await;
    let result = match init_result {
        Ok(r) => r,
        Err(e) => {
            handle.kill().await;
            return Err(format!("LSP initialize failed: {}", e));
        }
    };
    transport.notify("initialized", json!({})).await?;

    Ok(Arc::new(LspClient {
        project_id: project_id.to_string(),
        language_id: language_id.to_string(),
        transport,
        handle,
        stopping,
        capabilities: result.get("capabilities").cloned().unwrap_or(Value::Null),
        server_info: result.get("serverInfo").cloned().unwrap_or(Value::Null),
    }))
}

pub async fn cmd_lsp_start(
    state: &AppState,
    project_id: String,
    language_id: String,
    root_path: String,
) -> Result<LspServerInfo, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;
    let target = match &project.connection {
        crate::Connection::Local { .. } => SpawnTarget::Local,
        crate::Connection::Ssh { .. } => {
            let session = {
                let connections = state.ssh_connections.lock().await;
                connections
                    .get(&project_id)
                    .map(|conn| Arc::clone(&conn.session))
            };
            match session {
                Some(s) => SpawnTarget::Remote(s),
                None => return Err("SSH session is not connected".to_string()),
            }
        }
    };
    start_server(
        Some(state.event_bus.clone()),
        state.lsp_manager.clone(),
        &project_id,
        &language_id,
        &root_path,
        target,
    )
    .await
}

#[tauri::command]
pub async fn lsp_start(
    project_id: String,
    language_id: String,
    root_path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<LspServerInfo, String> {
    crate::commands::lsp_start(state.inner().as_ref(), project_id, language_id, root_path).await
}

pub async fn cmd_lsp_request(
    state: &AppState,
    project_id: String,
    language_id: String,
    method: String,
    params: Value,
) -> Result<Value, String> {
    let client = state
        .lsp_manager
        .get(&project_id, &language_id)
        .await
        .ok_or("Language server is not running")?;
    client.transport.request(&method, params).await
}

#[tauri::command]
pub async fn lsp_request(
    project_id: String,
    language_id: String,
    method: String,
    params: Value,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Value, String> {
    crate::commands::lsp_request(state.inner().as_ref(), project_id, language_id, method, params).await
}

pub async fn cmd_lsp_notify(
    state: &AppState,
    project_id: String,
    language_id: String,
    method: String,
    params: Value,
) -> Result<(), String> {
    let client = state
        .lsp_manager
        .get(&project_id, &language_id)
        .await
        .ok_or("Language server is not running")?;
    client.transport.notify(&method, params).await
}

#[tauri::command]
pub async fn lsp_notify(
    project_id: String,
    language_id: String,
    method: String,
    params: Value,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::lsp_notify(state.inner().as_ref(), project_id, language_id, method, params).await
}

pub async fn cmd_lsp_stop(
    state: &AppState,
    project_id: String,
    language_id: String,
) -> Result<(), String> {
    if let Some(client) = state.lsp_manager.remove(&project_id, &language_id).await {
        client.shutdown().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn lsp_stop(
    project_id: String,
    language_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::lsp_stop(state.inner().as_ref(), project_id, language_id).await
}

pub async fn cmd_lsp_list(state: &AppState) -> Result<Vec<LspServerInfo>, String> {
    Ok(state
        .lsp_manager
        .list()
        .await
        .iter()
        .map(|c| c.info())
        .collect())
}

#[tauri::command]
pub async fn lsp_list(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<LspServerInfo>, String> {
    crate::commands::lsp_list(state.inner().as_ref()).await
}

pub async fn cmd_lsp_server_available(
    state: &AppState,
    project_id: String,
    language_id: String,
) -> Result<bool, String> {
    let Some(spec) = registry::server_for_language(&language_id) else {
        return Ok(false);
    };
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;
    match &project.connection {
        crate::Connection::Local { .. } => Ok(registry::resolve_on_path(spec.command).is_some()),
        crate::Connection::Ssh { .. } => {
            let session = {
                let connections = state.ssh_connections.lock().await;
                connections
                    .get(&project_id)
                    .map(|conn| Arc::clone(&conn.session))
            };
            match session {
                Some(s) => Ok(remote::remote_command_available(&s, spec.command).await),
                None => Ok(false),
            }
        }
    }
}

#[tauri::command]
pub async fn lsp_server_available(
    project_id: String,
    language_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    crate::commands::lsp_server_available(state.inner().as_ref(), project_id, language_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn smoke_typescript_language_server() {
        let manager = Arc::new(LspManager::default());
        let cwd = std::env::current_dir().unwrap();
        let root = cwd.parent().unwrap().to_string_lossy().to_string();
        let info = start_server(
            None,
            manager.clone(),
            "test-project",
            "typescript",
            &root,
            SpawnTarget::Local,
        )
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

    #[tokio::test]
    #[ignore]
    async fn smoke_ruby_lsp() {
        let manager = Arc::new(LspManager::default());
        let cwd = std::env::current_dir().unwrap();
        let root = cwd.parent().unwrap().to_string_lossy().to_string();
        let info = start_server(
            None,
            manager.clone(),
            "test-project",
            "ruby",
            &root,
            SpawnTarget::Local,
        )
        .await
        .unwrap();
        assert!(info.capabilities.is_object());
        let client = manager.get("test-project", "ruby").await.unwrap();
        client
            .transport
            .notify(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": "file:///tmp/agent-ide-lsp-smoke.rb",
                        "languageId": "ruby",
                        "version": 1,
                        "text": "def greet(name)\n  puts name\nend\ngreet(\"hi\")\n"
                    }
                }),
            )
            .await
            .unwrap();
        let symbols = client
            .transport
            .request(
                "textDocument/documentSymbol",
                json!({
                    "textDocument": { "uri": "file:///tmp/agent-ide-lsp-smoke.rb" }
                }),
            )
            .await;
        assert!(symbols.is_ok(), "documentSymbol failed: {:?}", symbols.err());
        client.shutdown().await;
        manager.remove("test-project", "ruby").await;
    }
}
