use git2::Repository;
use russh::keys::PrivateKeyWithHashAlg;
use russh::*;
use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Connection {
    Local { path: String },
    Ssh {
        host: String,
        port: u16,
        username: String,
        auth_method: String,
        key_path: Option<String>,
        password: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub connection: Connection,
    pub worktrees: Vec<Worktree>,
    pub active_worktree_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    pub id: String,
    pub branch: String,
    pub path: String,
    pub is_main: bool,
    pub status: String,
    pub ahead: i32,
    pub behind: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshDirEntry {
    pub name: String,
    pub is_dir: bool,
}

pub struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub struct SshConnection {
    pub session: client::Handle<ClientHandler>,
    pub sftp: SftpSession,
}

pub struct AppState {
    pub ssh_connections: Mutex<HashMap<String, SshConnection>>,
}

#[tauri::command]
fn save_projects(projects: Vec<Project>, app_handle: tauri::AppHandle) -> Result<(), String> {
    let config_path = app_handle
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config path: {}", e))?;

    std::fs::create_dir_all(&config_path)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    let file_path = config_path.join("projects.json");
    let json = serde_json::to_string_pretty(&projects)
        .map_err(|e| format!("Failed to serialize projects: {}", e))?;

    std::fs::write(&file_path, json)
        .map_err(|e| format!("Failed to write projects file: {}", e))?;

    Ok(())
}

#[tauri::command]
fn load_projects(app_handle: tauri::AppHandle) -> Result<Vec<Project>, String> {
    let config_path = app_handle
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config path: {}", e))?;

    let file_path = config_path.join("projects.json");

    if !file_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read projects file: {}", e))?;

    let projects: Vec<Project> = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse projects file: {}", e))?;

    Ok(projects)
}

#[tauri::command]
fn check_is_git_repo(path: String) -> Result<bool, String> {
    let git_path = Path::new(&path).join(".git");
    Ok(git_path.exists())
}

#[tauri::command]
fn git_init(path: String) -> Result<(), String> {
    Repository::init(&path).map_err(|e| format!("Failed to initialize git repository: {}", e))?;
    Ok(())
}

async fn connect_ssh(
    host: &str,
    port: u16,
    username: &str,
    auth_method: &str,
    key_path: Option<&str>,
    password: Option<&str>,
) -> Result<(client::Handle<ClientHandler>, SftpSession), String> {
    let config = Arc::new(client::Config::default());

    let mut session = tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(config, (host, port), ClientHandler),
    )
    .await
    .map_err(|_| "Connection timed out".to_string())?
    .map_err(|e| format!("Failed to connect: {}", e))?;

    match auth_method {
        "key" => {
            let kp = key_path.ok_or("Key path is required for key authentication")?;
            let key = russh_keys::load_secret_key(kp, None)
                .map_err(|e| format!("Failed to load private key: {}", e))?;
            let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key), None);
            let auth_result = session
                .authenticate_publickey(username.to_string(), key_with_hash)
                .await
                .map_err(|e| format!("Key authentication failed: {}", e))?;
            if !auth_result.success() {
                return Err("Key authentication rejected".to_string());
            }
            let channel = session
                .channel_open_session()
                .await
                .map_err(|e| format!("Failed to open channel: {}", e))?;
            let stream = channel.into_stream();
            let sftp = SftpSession::new(stream)
                .await
                .map_err(|e| format!("Failed to initialize SFTP: {}", e))?;
            Ok((session, sftp))
        }
        "password" => {
            let pwd = password.ok_or("Password is required for password authentication")?;
            let auth_result = session
                .authenticate_password(username.to_string(), pwd.to_string())
                .await
                .map_err(|e| format!("Password authentication failed: {}", e))?;
            if !auth_result.success() {
                return Err("Password authentication rejected".to_string());
            }
            let channel = session
                .channel_open_session()
                .await
                .map_err(|e| format!("Failed to open channel: {}", e))?;
            let stream = channel.into_stream();
            let sftp = SftpSession::new(stream)
                .await
                .map_err(|e| format!("Failed to initialize SFTP: {}", e))?;
            Ok((session, sftp))
        }
        _ => Err(format!("Unsupported auth method: {}", auth_method)),
    }
}

#[tauri::command]
async fn ssh_test_connection(
    host: String,
    port: u16,
    username: String,
    auth_method: String,
    key_path: Option<String>,
    password: Option<String>,
) -> Result<String, String> {
    let (session, _sftp) = connect_ssh(
        &host,
        port,
        &username,
        &auth_method,
        key_path.as_deref(),
        password.as_deref(),
    )
    .await?;
    let _ = session.disconnect(Disconnect::ByApplication, "", "en");
    Ok("Connection successful".to_string())
}

#[tauri::command]
async fn ssh_connect(
    project_id: String,
    host: String,
    port: u16,
    username: String,
    auth_method: String,
    key_path: Option<String>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    {
        let connections = state.ssh_connections.lock().await;
        if connections.contains_key(&project_id) {
            return Ok(());
        }
    }

    let (session, sftp) = connect_ssh(
        &host,
        port,
        &username,
        &auth_method,
        key_path.as_deref(),
        password.as_deref(),
    )
    .await?;

    let mut connections = state.ssh_connections.lock().await;
    connections.insert(project_id, SshConnection { session, sftp });

    Ok(())
}

#[tauri::command]
async fn ssh_disconnect(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut connections = state.ssh_connections.lock().await;
    if let Some(conn) = connections.remove(&project_id) {
        let _ = conn.session.disconnect(Disconnect::ByApplication, "", "en");
    }
    Ok(())
}

#[tauri::command]
async fn ssh_list_directory(
    project_id: String,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SshDirEntry>, String> {
    let connections = state.ssh_connections.lock().await;
    let conn = connections
        .get(&project_id)
        .ok_or("No SSH connection found for this project")?;

    let entries = conn
        .sftp
        .read_dir(&path)
        .await
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    let result: Vec<SshDirEntry> = entries
        .into_iter()
        .filter(|e| e.file_name() != "." && e.file_name() != "..")
        .map(|e| SshDirEntry {
            name: e.file_name().clone(),
            is_dir: e.metadata().is_dir(),
        })
        .collect();

    Ok(result)
}

#[tauri::command]
async fn ssh_check_git(
    project_id: String,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let connections = state.ssh_connections.lock().await;
    let conn = connections
        .get(&project_id)
        .ok_or("No SSH connection found for this project")?;

    let git_path = format!("{}/.git", path.trim_end_matches('/'));

    match conn.sftp.metadata(&git_path).await {
        Ok(meta) => Ok(meta.is_dir()),
        Err(_) => Ok(false),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState {
            ssh_connections: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            save_projects,
            load_projects,
            check_is_git_repo,
            git_init,
            ssh_test_connection,
            ssh_connect,
            ssh_disconnect,
            ssh_list_directory,
            ssh_check_git,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
