use git2::Repository;
use russh::keys::agent::client::AgentClient;
use russh::keys::{PrivateKeyWithHashAlg, PublicKey};
use russh::*;
use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Connection {
    Local { path: String },
    Ssh {
        host: String,
        port: u16,
        username: String,
        #[serde(rename = "authMethod")]
        auth_method: String,
        #[serde(rename = "keyPath")]
        key_path: Option<String>,
        password: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub connection: Connection,
    pub worktrees: Vec<Worktree>,
    pub active_worktree_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    pub sftp: Option<SftpSession>,
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

fn one_password_agent_socket() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let legacy = PathBuf::from(&home)
                .join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock");
            info!("one_password_agent_socket: checking legacy path {:?}", legacy);
            if legacy.exists() {
                info!("one_password_agent_socket: found legacy path");
                return Some(legacy);
            }
            let symlink = PathBuf::from(&home).join(".1password/agent.sock");
            info!("one_password_agent_socket: checking symlink path {:?}", symlink);
            if symlink.exists() {
                info!("one_password_agent_socket: found symlink path");
                return Some(symlink);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let socket = PathBuf::from(&home).join(".1password/agent.sock");
            info!("one_password_agent_socket: checking linux home path {:?}", socket);
            if socket.exists() {
                info!("one_password_agent_socket: found linux home path");
                return Some(socket);
            }
        }
        if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
            let socket = PathBuf::from(&xdg).join("1password/agent.sock");
            info!("one_password_agent_socket: checking xdg runtime path {:?}", socket);
            if socket.exists() {
                info!("one_password_agent_socket: found xdg runtime path");
                return Some(socket);
            }
        }
    }

    info!("one_password_agent_socket: no 1Password socket found");
    None
}

async fn connect_ssh(
    host: &str,
    port: u16,
    username: &str,
    auth_method: &str,
    key_path: Option<&str>,
    password: Option<&str>,
) -> Result<(client::Handle<ClientHandler>, Option<SftpSession>), String> {
    connect_ssh_with_sftp(host, port, username, auth_method, key_path, password, true).await
}

async fn connect_ssh_with_sftp(
    host: &str,
    port: u16,
    username: &str,
    auth_method: &str,
    key_path: Option<&str>,
    password: Option<&str>,
    init_sftp: bool,
) -> Result<(client::Handle<ClientHandler>, Option<SftpSession>), String> {
    info!("connect_ssh: host={} port={} username={} auth_method={}", host, port, username, auth_method);
    let config = Arc::new(client::Config::default());

    let connect_timeout = if auth_method == "agent" {
        Duration::from_secs(120)
    } else {
        Duration::from_secs(15)
    };

    info!("connect_ssh: starting TCP connection with timeout {:?}", connect_timeout);
    let mut session = tokio::time::timeout(
        connect_timeout,
        client::connect(config, (host, port), ClientHandler),
    )
    .await
    .map_err(|_| {
        warn!("connect_ssh: TCP connection timed out");
        "Connection timed out".to_string()
    })?
    .map_err(|e| {
        warn!("connect_ssh: TCP connection failed: {}", e);
        format!("Failed to connect: {}", e)
    })?;
    info!("connect_ssh: TCP connection established");

    match auth_method {
        "key" => {
            info!("connect_ssh: starting key auth");
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
            info!("connect_ssh: key auth succeeded");
        }
        "agent" => {
            info!("connect_ssh: starting agent auth");
            let agent_path = one_password_agent_socket()
                .or_else(|| std::env::var("SSH_AUTH_SOCK").ok().filter(|s| !s.is_empty()).map(PathBuf::from))
                .ok_or("No 1Password agent socket found and SSH_AUTH_SOCK is not set")?;

            info!("connect_ssh: selected agent socket {:?}", agent_path);
            info!("connect_ssh: connecting to agent socket");
            let stream = UnixStream::connect(&agent_path)
                .await
                .map_err(|e| {
                    warn!("connect_ssh: failed to connect to agent socket: {}", e);
                    format!("Failed to connect to SSH agent socket: {}", e)
                })?;
            info!("connect_ssh: agent socket connected");
            let mut agent = AgentClient::connect(stream);
            info!("connect_ssh: requesting agent identities");

            let identities = agent
                .request_identities()
                .await
                .map_err(|e| {
                    warn!("connect_ssh: request_identities failed: {}", e);
                    format!("Failed to get identities from SSH agent: {}", e)
                })?;
            info!("connect_ssh: {} identities returned", identities.len());
            if identities.is_empty() {
                warn!("connect_ssh: agent has no identities");
                return Err("SSH agent has no keys. If you use 1Password, make sure it is unlocked and the SSH agent is enabled.".to_string());
            }

            let mut authenticated = false;
            let mut last_error: Option<String> = None;
            for key in &identities {
                let comment = key.comment();
                info!("connect_ssh: trying key '{}'", comment);
                let result = session
                    .authenticate_publickey_with(username.to_string(), key.clone(), None, &mut agent)
                    .await;
                match result {
                    Ok(auth) if auth.success() => {
                        info!("connect_ssh: key '{}' accepted", comment);
                        authenticated = true;
                        break;
                    }
                    Ok(_) => {
                        warn!("connect_ssh: key '{}' not accepted by server", comment);
                    }
                    Err(e) => {
                        warn!("connect_ssh: key '{}' error: {}", comment, e);
                        last_error = Some(format!("{}", e));
                    }
                }
            }

            if !authenticated {
                warn!("connect_ssh: no agent key accepted");
                return Err(last_error.unwrap_or_else(|| {
                    "SSH agent authentication rejected. None of the available keys were accepted by the server.".to_string()
                }));
            }
            info!("connect_ssh: agent auth succeeded");
        }
        "password" => {
            info!("connect_ssh: starting password auth");
            let pwd = password.ok_or("Password is required for password authentication")?;
            let auth_result = session
                .authenticate_password(username.to_string(), pwd.to_string())
                .await
                .map_err(|e| format!("Password authentication failed: {}", e))?;
            if !auth_result.success() {
                return Err("Password authentication rejected".to_string());
            }
            info!("connect_ssh: password auth succeeded");
        }
        _ => return Err(format!("Unsupported auth method: {}", auth_method)),
    }

    info!("connect_ssh: opening SSH channel");
    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("Failed to open channel: {}", e))?;
    info!("connect_ssh: channel opened");

    if !init_sftp {
        info!("connect_ssh: skipping SFTP init");
        return Ok((session, None));
    }

    info!("connect_ssh: requesting sftp subsystem");
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| {
            warn!("connect_ssh: request_subsystem failed: {}", e);
            format!("Failed to request SFTP subsystem: {}", e)
        })?;
    info!("connect_ssh: sftp subsystem requested");

    info!("connect_ssh: initializing SFTP with 10s timeout");
    let stream = channel.into_stream();
    let sftp = tokio::time::timeout(
        Duration::from_secs(10),
        SftpSession::new(stream),
    )
    .await
    .map_err(|_| {
        warn!("connect_ssh: SFTP initialization timed out");
        "SFTP initialization timed out".to_string()
    })?
    .map_err(|e| {
        warn!("connect_ssh: SFTP initialization failed: {}", e);
        format!("Failed to initialize SFTP: {}", e)
    })?;
    info!("connect_ssh: SFTP initialized");
    Ok((session, Some(sftp)))
}

async fn list_ssh_agent_keys() -> Result<Vec<String>, String> {
    info!("list_ssh_agent_keys: starting");
    let mut last_error: Option<String> = None;

    let sockets: Vec<Option<PathBuf>> = vec![
        one_password_agent_socket(),
        std::env::var("SSH_AUTH_SOCK").ok().filter(|s| !s.is_empty()).map(PathBuf::from),
    ];

    for socket in sockets.into_iter().flatten() {
        info!("list_ssh_agent_keys: trying socket {:?}", socket);
        match UnixStream::connect(&socket).await {
            Ok(stream) => {
                info!("list_ssh_agent_keys: connected to {:?}", socket);
                match AgentClient::connect(stream).request_identities().await {
                    Ok(keys) => {
                        info!("list_ssh_agent_keys: got {} keys from {:?}", keys.len(), socket);
                        let comments: Vec<String> = keys
                            .iter()
                            .filter_map(|k| {
                                let c = k.comment();
                                if c.is_empty() { None } else { Some(c.to_string()) }
                            })
                            .collect();
                        return Ok(comments);
                    }
                    Err(e) => {
                        warn!("list_ssh_agent_keys: request_identities failed for {:?}: {}", socket, e);
                        last_error = Some(format!("Failed to list identities from {:?}: {}", socket, e));
                    }
                }
            }
            Err(e) => {
                warn!("list_ssh_agent_keys: failed to connect to {:?}: {}", socket, e);
                last_error = Some(format!("Failed to connect to {:?}: {}", socket, e));
            }
        }
    }

    warn!("list_ssh_agent_keys: no agent socket worked");
    Err(last_error.unwrap_or_else(|| "No SSH agent socket found.".to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshAgentInfo {
    pub auth_sock: Option<String>,
    pub socket_exists: bool,
    pub one_password_socket: Option<String>,
    pub one_password_socket_exists: bool,
    pub agent_key_count: Option<usize>,
    pub agent_key_comments: Vec<String>,
    pub pub_key_count: usize,
    pub pub_key_comments: Vec<String>,
    pub error: Option<String>,
}

#[tauri::command]
async fn ssh_agent_info() -> Result<SshAgentInfo, String> {
    info!("ssh_agent_info: starting");
    let auth_sock = std::env::var("SSH_AUTH_SOCK").ok();
    let socket_exists = auth_sock
        .as_ref()
        .map(|p| std::path::Path::new(p).exists())
        .unwrap_or(false);

    let one_password_socket = one_password_agent_socket().map(|p| p.to_string_lossy().to_string());
    let one_password_socket_exists = one_password_socket
        .as_ref()
        .map(|p| std::path::Path::new(p).exists())
        .unwrap_or(false);
    info!("ssh_agent_info: auth_sock={:?} socket_exists={} one_password_socket={:?} one_password_socket_exists={}",
        auth_sock, socket_exists, one_password_socket, one_password_socket_exists);

    let (agent_key_count, agent_key_comments, error) = match list_ssh_agent_keys().await {
        Ok(comments) => {
            info!("ssh_agent_info: found {} keys", comments.len());
            (Some(comments.len()), comments, None)
        }
        Err(e) => {
            warn!("ssh_agent_info: failed to list keys: {}", e);
            (None, vec![], Some(e))
        }
    };

    let mut pub_key_comments = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let ssh_dir = std::path::Path::new(&home).join(".ssh");
        if ssh_dir.is_dir() {
            for entry in std::fs::read_dir(&ssh_dir).ok().into_iter().flatten() {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "pub") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(key) = PublicKey::from_openssh(&content) {
                                let comment = key.comment();
                                if !comment.is_empty() {
                                    pub_key_comments.push(comment.to_string());
                                } else {
                                    pub_key_comments.push(path.file_name().unwrap_or_default().to_string_lossy().to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(SshAgentInfo {
        auth_sock,
        socket_exists,
        one_password_socket,
        one_password_socket_exists,
        agent_key_count,
        agent_key_comments,
        pub_key_count: pub_key_comments.len(),
        pub_key_comments,
        error,
    })
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
    info!("ssh_test_connection: starting connect with SFTP");
    let (session, sftp) = connect_ssh(
        &host,
        port,
        &username,
        &auth_method,
        key_path.as_deref(),
        password.as_deref(),
    )
    .await?;
    info!("ssh_test_connection: connect succeeded (sftp={}), disconnecting", sftp.is_some());
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        session.disconnect(Disconnect::ByApplication, "", "en"),
    )
    .await;
    info!("ssh_test_connection: disconnect done");
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
    info!("ssh_connect: project_id={} host={} port={} username={} auth_method={}", project_id, host, port, username, auth_method);
    {
        let connections = state.ssh_connections.lock().await;
        if connections.contains_key(&project_id) {
            info!("ssh_connect: connection already exists");
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

    info!("ssh_connect: connect succeeded, storing connection (sftp={})", sftp.is_some());
    let mut connections = state.ssh_connections.lock().await;
    connections.insert(project_id, SshConnection { session, sftp });

    Ok(())
}

#[tauri::command]
async fn ssh_disconnect(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    info!("ssh_disconnect: project_id={}", project_id);
    let mut connections = state.ssh_connections.lock().await;
    if let Some(conn) = connections.remove(&project_id) {
        info!("ssh_disconnect: disconnecting session");
        let _ = conn.session.disconnect(Disconnect::ByApplication, "", "en").await;
        info!("ssh_disconnect: session disconnected");
    }
    Ok(())
}

#[tauri::command]
async fn ssh_list_directory(
    project_id: String,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SshDirEntry>, String> {
    info!("ssh_list_directory: project_id={} path={}", project_id, path);
    let connections = state.ssh_connections.lock().await;
    let conn = connections
        .get(&project_id)
        .ok_or("No SSH connection found for this project")?;

    let sftp = conn
        .sftp
        .as_ref()
        .ok_or("SFTP is not available for this connection")?;

    let entries = sftp
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

    info!("ssh_list_directory: returning {} entries", result.len());
    Ok(result)
}

#[tauri::command]
async fn ssh_check_git(
    project_id: String,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    info!("ssh_check_git: project_id={} path={}", project_id, path);
    let connections = state.ssh_connections.lock().await;
    let conn = connections
        .get(&project_id)
        .ok_or("No SSH connection found for this project")?;

    let sftp = conn
        .sftp
        .as_ref()
        .ok_or("SFTP is not available for this connection")?;

    let git_path = format!("{}/.git", path.trim_end_matches('/'));

    let is_git = match sftp.metadata(&git_path).await {
        Ok(meta) => meta.is_dir(),
        Err(_) => false,
    };
    info!("ssh_check_git: is_git={}", is_git);
    Ok(is_git)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

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
            ssh_agent_info,
            ssh_test_connection,
            ssh_connect,
            ssh_disconnect,
            ssh_list_directory,
            ssh_check_git,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
