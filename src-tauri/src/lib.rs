use git2::{BranchType, Repository};
use russh::keys::agent::client::AgentClient;
use russh::keys::{PrivateKeyWithHashAlg, PublicKey};
use russh::*;
use russh_sftp::client::SftpSession;
use tokio::io::AsyncWriteExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tauri::Emitter;
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
        #[serde(skip)]
        password: Option<String>,
        path: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub id: String,
    pub branch: String,
    pub path: String,
    pub is_main: bool,
    pub status: String,
    pub ahead: i32,
    pub behind: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub is_remote: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub is_dir: bool,
    pub size: u64,
}

#[async_trait::async_trait]
pub trait FileSystemProvider: Send + Sync {
    async fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, String>;
    async fn read_file(&self, path: &str) -> Result<String, String>;
    async fn write_file(&self, path: &str, content: &str) -> Result<(), String>;
    async fn stat(&self, path: &str) -> Result<FileStat, String>;
    async fn mkdir(&self, path: &str) -> Result<(), String>;
    async fn rm(&self, path: &str, recursive: bool) -> Result<(), String>;
    async fn mv(&self, from: &str, to: &str) -> Result<(), String>;
    async fn exists(&self, path: &str) -> bool;
}

pub struct LocalFileSystem;

#[async_trait::async_trait]
impl FileSystemProvider for LocalFileSystem {
    async fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, String> {
        let entries = std::fs::read_dir(path).map_err(|e| format!("Failed to read directory: {}", e))?;
        let mut result = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let metadata = entry.metadata().map_err(|e| format!("Failed to read metadata: {}", e))?;
            result.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
            });
        }
        result.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name))
        });
        Ok(result)
    }

    async fn read_file(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        std::fs::write(path, content).map_err(|e| format!("Failed to write file: {}", e))
    }

    async fn stat(&self, path: &str) -> Result<FileStat, String> {
        let metadata = std::fs::metadata(path).map_err(|e| format!("Failed to stat: {}", e))?;
        Ok(FileStat {
            is_dir: metadata.is_dir(),
            size: metadata.len(),
        })
    }

    async fn mkdir(&self, path: &str) -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(|e| format!("Failed to create directory: {}", e))
    }

    async fn rm(&self, path: &str, recursive: bool) -> Result<(), String> {
        let p = Path::new(path);
        if recursive {
            std::fs::remove_dir_all(p).map_err(|e| format!("Failed to remove: {}", e))
        } else if p.is_dir() {
            std::fs::remove_dir(p).map_err(|e| format!("Failed to remove directory: {}", e))
        } else {
            std::fs::remove_file(p).map_err(|e| format!("Failed to remove file: {}", e))
        }
    }

    async fn mv(&self, from: &str, to: &str) -> Result<(), String> {
        std::fs::rename(from, to).map_err(|e| format!("Failed to move: {}", e))
    }

    async fn exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }
}

pub struct SftpFileSystem {
    pub project_id: String,
    pub state: Arc<AppState>,
}

impl SftpFileSystem {
    async fn get_sftp(&self) -> Result<Arc<SftpSession>, String> {
        let connections = self.state.ssh_connections.lock().await;
        let conn = connections
            .get(&self.project_id)
            .ok_or("No SSH connection found for this project")?;
        conn.sftp
            .as_ref()
            .ok_or("SFTP is not available for this connection".to_string())
            .map(Arc::clone)
    }

    async fn resolve_path(&self, path: &str) -> Result<String, String> {
        if path.starts_with('/') {
            return Ok(path.to_string());
        }
        if path.starts_with("~/") || path == "~" {
            let connections = self.state.ssh_connections.lock().await;
            let conn = connections
                .get(&self.project_id)
                .ok_or("No SSH connection found for this project")?;
            let mut channel = conn.session
                .channel_open_session()
                .await
                .map_err(|e| format!("Failed to open channel: {}", e))?;
            channel
                .exec(false, "echo $HOME")
                .await
                .map_err(|e| format!("Failed to execute command: {}", e))?;
            let mut home = String::new();
            loop {
                if let Some(msg) = channel.wait().await {
                    match msg {
                        russh::ChannelMsg::Data { data } => {
                            home.push_str(&String::from_utf8_lossy(&data));
                        }
                        russh::ChannelMsg::ExitStatus { .. } => break,
                        _ => {}
                    }
                }
            }
            let home = home.trim();
            let rel = path.trim_start_matches('~').trim_start_matches('/');
            return Ok(format!("{}/{}", home, rel));
        }
        let connections = self.state.ssh_connections.lock().await;
        let conn = connections
            .get(&self.project_id)
            .ok_or("No SSH connection found for this project")?;
        let mut channel = conn.session
            .channel_open_session()
            .await
            .map_err(|e| format!("Failed to open channel: {}", e))?;
        channel
            .exec(false, "pwd")
            .await
            .map_err(|e| format!("Failed to execute command: {}", e))?;
        let mut cwd = String::new();
        loop {
            if let Some(msg) = channel.wait().await {
                match msg {
                    russh::ChannelMsg::Data { data } => {
                        cwd.push_str(&String::from_utf8_lossy(&data));
                    }
                    russh::ChannelMsg::ExitStatus { .. } => break,
                    _ => {}
                }
            }
        }
        Ok(format!("{}/{}", cwd.trim(), path))
    }
}

#[async_trait::async_trait]
impl FileSystemProvider for SftpFileSystem {
    async fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, String> {
        let resolved = self.resolve_path(path).await?;
        let sftp = self.get_sftp().await?;
        let entries = sftp
            .read_dir(&resolved)
            .await
            .map_err(|e| format!("Failed to read directory: {}", e))?;
        let mut result = Vec::new();
        for entry in entries {
            let name = entry.file_name().clone();
            if name == "." || name == ".." {
                continue;
            }
            let meta = entry.metadata();
            result.push(DirEntry {
                name,
                is_dir: meta.is_dir(),
                size: meta.len(),
            });
        }
        result.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name))
        });
        Ok(result)
    }

    async fn read_file(&self, path: &str) -> Result<String, String> {
        let resolved = self.resolve_path(path).await?;
        let sftp = self.get_sftp().await?;
        let content = sftp
            .read(&resolved)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;
        String::from_utf8(content).map_err(|e| format!("Invalid UTF-8 in file: {}", e))
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        let resolved = self.resolve_path(path).await?;
        let sftp = self.get_sftp().await?;

        // ensure parent dirs exist (SFTP can't create nested dirs in one call)
        let parts: Vec<&str> = resolved.split('/').filter(|s| !s.is_empty()).collect();
        let mut accum = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 { break; } // skip the file itself
            accum.push('/');
            accum.push_str(part);
            // ignore errors — directory might already exist
            let _ = sftp.create_dir(&accum).await;
        }

        // create() uses CREATE | TRUNCATE | WRITE (creates file if missing)
        let mut file = sftp
            .create(&resolved)
            .await
            .map_err(|e| format!("Failed to create file on remote: {}", e))?;
        // File implements AsyncWrite
        file.write_all(content.as_bytes())
            .await
            .map_err(|e| format!("Failed to write file: {}", e))?;
        file.flush().await.map_err(|e| format!("Failed to flush file: {}", e))?;
        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<FileStat, String> {
        let resolved = self.resolve_path(path).await?;
        let sftp = self.get_sftp().await?;
        let meta = sftp
            .metadata(&resolved)
            .await
            .map_err(|e| format!("Failed to stat: {}", e))?;
        Ok(FileStat {
            is_dir: meta.is_dir(),
            size: meta.len(),
        })
    }

    async fn mkdir(&self, path: &str) -> Result<(), String> {
        let resolved = self.resolve_path(path).await?;
        let sftp = self.get_sftp().await?;
        sftp
            .create_dir(&resolved)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))
    }

    async fn rm(&self, path: &str, recursive: bool) -> Result<(), String> {
        let resolved = self.resolve_path(path).await?;
        let sftp = self.get_sftp().await?;
        let stat = sftp
            .metadata(&resolved)
            .await
            .map_err(|e| format!("Failed to stat: {}", e))?;
        if stat.is_dir() {
            if recursive {
                sftp.remove_dir(&resolved)
                    .await
                    .map_err(|e| format!("Failed to remove directory: {}", e))
            } else {
                Err("Cannot remove non-empty directory without recursive flag".to_string())
            }
        } else {
            sftp.remove_file(&resolved)
                .await
                .map_err(|e| format!("Failed to remove file: {}", e))
        }
    }

    async fn mv(&self, from: &str, to: &str) -> Result<(), String> {
        let from_resolved = self.resolve_path(from).await?;
        let to_resolved = self.resolve_path(to).await?;
        let sftp = self.get_sftp().await?;
        sftp
            .rename(&from_resolved, &to_resolved)
            .await
            .map_err(|e| format!("Failed to move: {}", e))
    }

    async fn exists(&self, path: &str) -> bool {
        let resolved = match self.resolve_path(path).await {
            Ok(r) => r,
            Err(_) => return false,
        };
        let sftp = match self.get_sftp().await {
            Ok(s) => s,
            Err(_) => return false,
        };
        sftp.metadata(&resolved).await.is_ok()
    }
}

async fn get_fs_provider(project_id: &str, state: &Arc<AppState>) -> Result<Box<dyn FileSystemProvider>, String> {
    let projects = {
        let app_handle = state.app_handle.lock().unwrap().clone().ok_or("App handle not available")?;
        load_projects(app_handle)?
    };
    let project = projects.iter().find(|p| p.id == project_id).ok_or("Project not found")?;

    match &project.connection {
        Connection::Local { .. } => Ok(Box::new(LocalFileSystem)),
        Connection::Ssh { .. } => Ok(Box::new(SftpFileSystem {
            project_id: project_id.to_string(),
            state: state.clone(),
        })),
    }
}

#[tauri::command]
async fn fs_read_dir(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<DirEntry>, String> {
    let provider = get_fs_provider(&project_id, &state.inner()).await?;
    provider.read_dir(&path).await
}

#[tauri::command]
async fn fs_read_file(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let provider = get_fs_provider(&project_id, &state.inner()).await?;
    provider.read_file(&path).await
}

#[tauri::command]
async fn fs_write_file(
    project_id: String,
    path: String,
    content: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let provider = get_fs_provider(&project_id, &state.inner()).await?;
    provider.write_file(&path, &content).await
}

#[tauri::command]
async fn fs_stat(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<FileStat, String> {
    let provider = get_fs_provider(&project_id, &state.inner()).await?;
    provider.stat(&path).await
}

#[tauri::command]
async fn fs_mkdir(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let provider = get_fs_provider(&project_id, &state.inner()).await?;
    provider.mkdir(&path).await
}

#[tauri::command]
async fn fs_rm(
    project_id: String,
    path: String,
    recursive: Option<bool>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let provider = get_fs_provider(&project_id, &state.inner()).await?;
    provider.rm(&path, recursive.unwrap_or(false)).await
}

#[tauri::command]
async fn fs_mv(
    project_id: String,
    from: String,
    to: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let provider = get_fs_provider(&project_id, &state.inner()).await?;
    provider.mv(&from, &to).await
}

#[tauri::command]
async fn fs_exists(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    let provider = get_fs_provider(&project_id, &state.inner()).await?;
    Ok(provider.exists(&path).await)
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatusEvent {
    pub project_id: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SshCredentials {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub key_path: Option<String>,
    pub password: Option<String>,
}

pub struct SshConnection {
    pub session: client::Handle<ClientHandler>,
    pub sftp: Option<Arc<SftpSession>>,
    pub credentials: SshCredentials,
    pub status: ConnectionStatus,
    pub reconnect_attempts: u32,
}

pub struct AppState {
    pub ssh_connections: Mutex<HashMap<String, SshConnection>>,
    pub app_handle: StdMutex<Option<tauri::AppHandle>>,
}

impl AppState {
    pub fn emit_status(&self, project_id: &str, status: ConnectionStatus, error: Option<String>) {
        if let Some(handle) = self.app_handle.lock().unwrap().as_ref() {
            let _ = handle.emit("ssh_connection_status", ConnectionStatusEvent {
                project_id: project_id.to_string(),
                status: match status {
                    ConnectionStatus::Connected => "connected".to_string(),
                    ConnectionStatus::Disconnected => "disconnected".to_string(),
                    ConnectionStatus::Reconnecting => "reconnecting".to_string(),
                    ConnectionStatus::Error => "error".to_string(),
                },
                error,
            });
        }
    }
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

fn run_git_command(worktree_path: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute git command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git command failed: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn compute_ahead_behind(repo: &Repository, branch_name: &str) -> (i32, i32) {
    let head_branch = match repo.find_branch(branch_name, BranchType::Local) {
        Ok(b) => b,
        Err(_) => return (0, 0),
    };

    let head_commit = match head_branch.get().peel_to_commit() {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };

    let upstream_name = format!("origin/{}", branch_name);
    let upstream_branch = match repo.find_branch(&upstream_name, BranchType::Remote) {
        Ok(b) => b,
        Err(_) => return (0, 0),
    };

    let upstream_commit = match upstream_branch.get().peel_to_commit() {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };

    match repo.graph_ahead_behind(head_commit.id(), upstream_commit.id()) {
        Ok((ahead, behind)) => (ahead as i32, behind as i32),
        Err(_) => (0, 0),
    }
}

fn is_worktree_dirty(repo: &Repository) -> bool {
    if let Ok(statuses) = repo.statuses(None) {
        for entry in statuses.iter() {
            if entry.status() != git2::Status::CURRENT {
                return true;
            }
        }
    }
    false
}

fn list_worktrees_local(repo_path: &str) -> Result<Vec<WorktreeInfo>, String> {
    let repo = Repository::open(repo_path).map_err(|e| format!("Failed to open repository: {}", e))?;

    let mut result = Vec::new();

    let main_branch = if let Ok(head) = repo.head() {
        if head.is_branch() {
            head.shorthand().unwrap_or("main").to_string()
        } else {
            "main".to_string()
        }
    } else {
        "main".to_string()
    };

    let (ahead, behind) = compute_ahead_behind(&repo, &main_branch);
    let status = if is_worktree_dirty(&repo) { "dirty".to_string() } else { "clean".to_string() };

    result.push(WorktreeInfo {
        id: repo_path.split('/').filter(|s| !s.is_empty()).last().unwrap_or(repo_path).to_string(),
        branch: main_branch,
        path: repo_path.to_string(),
        is_main: true,
        status,
        ahead,
        behind,
    });

    let worktrees = repo.worktrees().map_err(|e| format!("Failed to list worktrees: {}", e))?;

    for wt_name_opt in worktrees.iter() {
        let wt_name = wt_name_opt.ok_or("Failed to read worktree name")?;

        let wt = repo.find_worktree(wt_name).map_err(|e| format!("Failed to find worktree: {}", e))?;

        let wt_path = wt.path().to_str().unwrap_or("").to_string();

        let branch = if let Ok(wt_repo) = Repository::open(&wt_path) {
            if let Ok(head) = wt_repo.head() {
                if head.is_branch() {
                    head.shorthand().unwrap_or(wt_name).to_string()
                } else {
                    wt_name.to_string()
                }
            } else {
                wt_name.to_string()
            }
        } else {
            wt_name.to_string()
        };

        let (ahead, behind) = compute_ahead_behind(&repo, &branch);

        let status = if is_worktree_dirty(&repo) {
            "dirty".to_string()
        } else {
            "clean".to_string()
        };

        result.push(WorktreeInfo {
            id: wt_name.to_string(),
            branch,
            path: wt_path,
            is_main: false,
            status,
            ahead,
            behind,
        });
    }

    Ok(result)
}

fn add_worktree_local(
    repo_path: &str,
    branch: &str,
    path: &str,
    new_branch: bool,
) -> Result<(), String> {
    if Path::new(path).exists() {
        return Err(format!("Path '{}' already exists", path));
    }

    if !new_branch {
        let repo = Repository::open(repo_path).map_err(|e| format!("Failed to open repository: {}", e))?;
        let branch_exists = repo.find_branch(branch, BranchType::Local).is_ok()
            || repo.find_branch(branch, BranchType::Remote).is_ok();
        if !branch_exists {
            return Err(format!("Branch '{}' does not exist", branch));
        }
    }

    let args = if new_branch {
        vec!["worktree", "add", path, "-b", branch]
    } else {
        vec!["worktree", "add", path, branch]
    };
    run_git_command(repo_path, &args)?;

    Ok(())
}

fn remove_worktree_local(repo_path: &str, worktree_path: &str, force: bool) -> Result<(), String> {
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(worktree_path);

    run_git_command(repo_path, &args)?;

    Ok(())
}

fn list_branches_local(repo_path: &str) -> Result<Vec<BranchInfo>, String> {
    let repo = Repository::open(repo_path).map_err(|e| format!("Failed to open repository: {}", e))?;

    let mut branches = Vec::new();

    let branches_iter = repo.branches(None).map_err(|e| format!("Failed to list branches: {}", e))?;
    for branch_result in branches_iter {
        let (branch, bt) = branch_result.map_err(|e| format!("Failed to read branch: {}", e))?;
        if let Some(name) = branch.name().map_err(|e| format!("Failed to get branch name: {}", e))? {
            branches.push(BranchInfo {
                name: name.to_string(),
                is_remote: bt == BranchType::Remote,
            });
        }
    }

    Ok(branches)
}

async fn run_git_command_ssh(
    project_id: &str,
    worktree_path: &str,
    args: &[&str],
    state: &Arc<AppState>,
) -> Result<String, String> {
    let connections = state.ssh_connections.lock().await;
    let conn = connections
        .get(project_id)
        .ok_or("No SSH connection found for this project")?;

    let cmd = format!("cd {} && git {}", worktree_path, args.join(" "));

    info!("run_git_command_ssh: executing '{}'", cmd);

    let mut channel = conn.session
        .channel_open_session()
        .await
        .map_err(|e| format!("Failed to open channel: {}", e))?;

    channel
        .exec(false, cmd.as_str())
        .await
        .map_err(|e| format!("Failed to execute command: {}", e))?;

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_status: Option<u32> = None;

    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => {
                stdout.push_str(&String::from_utf8_lossy(&data));
            }
            russh::ChannelMsg::ExtendedData { data, ext } => {
                if ext == 1 {
                    stderr.push_str(&String::from_utf8_lossy(&data));
                } else {
                    stdout.push_str(&String::from_utf8_lossy(&data));
                }
            }
            russh::ChannelMsg::Eof => {}
            russh::ChannelMsg::ExitStatus { exit_status: status } => {
                exit_status = Some(status);
            }
            russh::ChannelMsg::Close => {
                break;
            }
            _ => {}
        }
        if exit_status.is_some() {
            break;
        }
    }

    match exit_status {
        Some(0) => Ok(stdout.trim().to_string()),
        Some(code) => Err(format!("git command failed (exit {}): {}", code, stderr.trim())),
        None => Err("git command: no exit status received".to_string()),
    }
}

async fn list_worktrees_ssh(
    project_id: &str,
    repo_path: &str,
    state: &Arc<AppState>,
) -> Result<Vec<WorktreeInfo>, String> {
    let output = run_git_command_ssh(project_id, repo_path, &["worktree", "list", "--porcelain"], state).await?;

    let mut worktrees = Vec::new();
    let mut current_worktree: Option<WorktreeInfo> = None;

    for line in output.lines() {
        let line = line.trim();

        if line.starts_with("worktree ") {
            if let Some(wt) = current_worktree.take() {
                worktrees.push(wt);
            }
            let path = line.trim_start_matches("worktree ").to_string();
            let id = path.clone();
            current_worktree = Some(WorktreeInfo {
                id,
                branch: String::new(),
                path: path.clone(),
                is_main: false,
                status: "unknown".to_string(),
                ahead: 0,
                behind: 0,
            });
        } else if line.starts_with("branch ") {
            if let Some(ref mut wt) = current_worktree {
                let branch_ref = line.trim_start_matches("branch ");
                wt.branch = branch_ref
                    .trim_start_matches("refs/heads/")
                    .trim_start_matches("refs/remotes/")
                    .to_string();
            }
        } else if line == "bare" {
            if let Some(ref mut wt) = current_worktree {
                wt.is_main = true;
            }
        } else if line.starts_with("HEAD ") {
            if let Some(ref mut wt) = current_worktree {
                let head_ref = line.trim_start_matches("HEAD ");
                if wt.branch.is_empty() {
                    wt.branch = head_ref
                        .trim_start_matches("refs/heads/")
                        .trim_start_matches("refs/remotes/")
                        .to_string();
                }
            }
        }
    }

    if let Some(wt) = current_worktree {
        worktrees.push(wt);
    }

    if !worktrees.is_empty() {
        worktrees[0].is_main = true;
    }

    for wt in &mut worktrees {
        let status_output = run_git_command_ssh(project_id, &wt.path, &["status", "--porcelain"], state).await;
        wt.status = match status_output {
            Ok(output) => if output.is_empty() { "clean".to_string() } else { "dirty".to_string() },
            Err(_) => "unknown".to_string(),
        };

        let branch = &wt.branch;
        let ahead_output = run_git_command_ssh(
            project_id,
            &wt.path,
            &["rev-list", "--left-right", "--count", &format!("{}...origin/{}", branch, branch)],
            state,
        )
        .await;

        if let Ok(output) = ahead_output {
            let parts: Vec<&str> = output.split_whitespace().collect();
            if parts.len() == 2 {
                wt.ahead = parts[0].parse().unwrap_or(0);
                wt.behind = parts[1].parse().unwrap_or(0);
            }
        }
    }

    Ok(worktrees)
}

async fn add_worktree_ssh(
    project_id: &str,
    repo_path: &str,
    branch: &str,
    path: &str,
    new_branch: bool,
    state: &Arc<AppState>,
) -> Result<(), String> {
    let check_output = run_git_command_ssh(project_id, repo_path, &["ls-tree", "HEAD", path], state).await;
    if check_output.is_ok() {
        return Err(format!("Path '{}' already exists", path));
    }

    if new_branch {
        run_git_command_ssh(project_id, repo_path, &["worktree", "add", path, "-b", branch], state).await?;
    } else {
        run_git_command_ssh(project_id, repo_path, &["worktree", "add", path, branch], state).await?;
    }

    Ok(())
}

async fn remove_worktree_ssh(
    project_id: &str,
    repo_path: &str,
    worktree_path: &str,
    force: bool,
    state: &Arc<AppState>,
) -> Result<(), String> {
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(worktree_path);

    run_git_command_ssh(project_id, repo_path, &args, state).await?;

    Ok(())
}

async fn list_branches_ssh(
    project_id: &str,
    repo_path: &str,
    state: &Arc<AppState>,
) -> Result<Vec<BranchInfo>, String> {
    let mut branches = Vec::new();

    let local_output = run_git_command_ssh(project_id, repo_path, &["branch", "--list", "--format=%(refname:short)"], state).await?;
    for line in local_output.lines() {
        let name = line.trim();
        if !name.is_empty() {
            branches.push(BranchInfo {
                name: name.to_string(),
                is_remote: false,
            });
        }
    }

    let remote_output = run_git_command_ssh(project_id, repo_path, &["branch", "-r", "--list", "--format=%(refname:short)"], state).await?;
    for line in remote_output.lines() {
        let name = line.trim();
        if !name.is_empty() {
            branches.push(BranchInfo {
                name: name.to_string(),
                is_remote: true,
            });
        }
    }

    Ok(branches)
}

#[tauri::command]
fn git_worktree_list(project_id: String, app_handle: tauri::AppHandle) -> Result<Vec<WorktreeInfo>, String> {
    let projects = load_projects(app_handle)?;
    let project = projects.iter().find(|p| p.id == project_id).ok_or("Project not found")?;

    match &project.connection {
        Connection::Local { path } => list_worktrees_local(path),
        Connection::Ssh { .. } => Err("SSH worktree listing requires async execution. Use git_worktree_list_async instead.".to_string()),
    }
}

fn get_repo_path(project: &Project) -> String {
    match &project.connection {
        Connection::Local { path } => path.clone(),
        Connection::Ssh { path: Some(path), .. } => path.clone(),
        Connection::Ssh { username, .. } => {
            let worktree = project.worktrees.iter().find(|w| w.is_main).or(project.worktrees.first());
            worktree.map(|w| w.path.clone()).unwrap_or_else(|| {
                format!("{}/{}", username, project.name)
            })
        }
    }
}

#[tauri::command]
async fn git_worktree_list_async(
    project_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<WorktreeInfo>, String> {
    let projects = {
        let app_handle = state.app_handle.lock().unwrap().clone().ok_or("App handle not available")?;
        load_projects(app_handle)?
    };
    let project = projects.iter().find(|p| p.id == project_id).ok_or("Project not found")?;

    match &project.connection {
        Connection::Local { path } => list_worktrees_local(path),
        Connection::Ssh { .. } => {
            let repo_path = get_repo_path(project);
            list_worktrees_ssh(&project_id, &repo_path, &state).await
        }
    }
}

#[tauri::command]
async fn git_worktree_add_async(
    project_id: String,
    branch: String,
    path: String,
    new_branch: Option<bool>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let projects = {
        let app_handle = state.app_handle.lock().unwrap().clone().ok_or("App handle not available")?;
        load_projects(app_handle)?
    };
    let project = projects.iter().find(|p| p.id == project_id).ok_or("Project not found")?;

    let new_branch = new_branch.unwrap_or(false);

    match &project.connection {
        Connection::Local { path: repo_path } => add_worktree_local(repo_path, &branch, &path, new_branch),
        Connection::Ssh { .. } => {
            let repo_path = get_repo_path(project);
            add_worktree_ssh(&project_id, &repo_path, &branch, &path, new_branch, &state).await
        }
    }
}

#[tauri::command]
async fn git_worktree_remove_async(
    project_id: String,
    worktree_path: String,
    force: Option<bool>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let projects = {
        let app_handle = state.app_handle.lock().unwrap().clone().ok_or("App handle not available")?;
        load_projects(app_handle)?
    };
    let project = projects.iter().find(|p| p.id == project_id).ok_or("Project not found")?;

    let force = force.unwrap_or(false);

    match &project.connection {
        Connection::Local { path: repo_path } => remove_worktree_local(repo_path, &worktree_path, force),
        Connection::Ssh { .. } => {
            let repo_path = get_repo_path(project);
            remove_worktree_ssh(&project_id, &repo_path, &worktree_path, force, &state).await
        }
    }
}

#[tauri::command]
async fn git_branches_list_async(
    project_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<BranchInfo>, String> {
    let projects = {
        let app_handle = state.app_handle.lock().unwrap().clone().ok_or("App handle not available")?;
        load_projects(app_handle)?
    };
    let project = projects.iter().find(|p| p.id == project_id).ok_or("Project not found")?;

    match &project.connection {
        Connection::Local { path } => list_branches_local(path),
        Connection::Ssh { .. } => {
            let repo_path = get_repo_path(project);
            list_branches_ssh(&project_id, &repo_path, &state).await
        }
    }
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
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    info!("ssh_connect: project_id={} host={} port={} username={} auth_method={}", project_id, host, port, username, auth_method);
    {
        let connections = state.ssh_connections.lock().await;
        if let Some(conn) = connections.get(&project_id) {
            match conn.status {
                ConnectionStatus::Connected => {
                    info!("ssh_connect: connection already exists and is connected");
                    return Ok(());
                }
                ConnectionStatus::Reconnecting => {
                    info!("ssh_connect: connection is reconnecting, waiting...");
                    drop(connections);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let connections = state.ssh_connections.lock().await;
                    if let Some(c) = connections.get(&project_id) {
                        if c.status == ConnectionStatus::Connected {
                            return Ok(());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let credentials = SshCredentials {
        host: host.clone(),
        port,
        username: username.clone(),
        auth_method: auth_method.clone(),
        key_path: key_path.clone(),
        password: password.clone(),
    };

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
    connections.insert(project_id.clone(), SshConnection {
        session,
        sftp: sftp.map(Arc::new),
        credentials,
        status: ConnectionStatus::Connected,
        reconnect_attempts: 0,
    });
    state.emit_status(&project_id, ConnectionStatus::Connected, None);

    Ok(())
}

#[tauri::command]
async fn ssh_disconnect(
    project_id: String,
    state: tauri::State<'_, Arc<AppState>>,
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
    state: tauri::State<'_, Arc<AppState>>,
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
    state: tauri::State<'_, Arc<AppState>>,
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

#[tauri::command]
fn ssh_store_password(project_id: String, password: String) -> Result<(), String> {
    info!("ssh_store_password: project_id={}", project_id);
    let entry = keyring::Entry::new("agent-ide-ssh", &project_id)
        .map_err(|e| format!("Failed to create keychain entry: {}", e))?;
    entry
        .set_password(&password)
        .map_err(|e| format!("Failed to store password: {}", e))?;
    Ok(())
}

#[tauri::command]
fn ssh_get_password(project_id: String) -> Result<Option<String>, String> {
    info!("ssh_get_password: project_id={}", project_id);
    let entry = keyring::Entry::new("agent-ide-ssh", &project_id)
        .map_err(|e| format!("Failed to create keychain entry: {}", e))?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to retrieve password: {}", e)),
    }
}

#[tauri::command]
fn ssh_delete_password(project_id: String) -> Result<(), String> {
    info!("ssh_delete_password: project_id={}", project_id);
    let entry = keyring::Entry::new("agent-ide-ssh", &project_id)
        .map_err(|e| format!("Failed to create keychain entry: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to delete password: {}", e)),
    }
}

async fn start_health_check(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;

        let project_ids: Vec<String> = {
            let connections = state.ssh_connections.lock().await;
            connections.keys().cloned().collect()
        };

        for project_id in project_ids {
            check_and_reconnect(&project_id, &state).await;
        }
    }
}

async fn check_and_reconnect(project_id: &str, state: &Arc<AppState>) {
    let needs_reconnect = {
        let connections = state.ssh_connections.lock().await;
        if let Some(conn) = connections.get(project_id) {
            if conn.status != ConnectionStatus::Connected {
                return;
            }
            // Test SFTP connection
            if let Some(sftp) = &conn.sftp {
                match tokio::time::timeout(Duration::from_secs(5), sftp.read_dir("/")).await {
                    Ok(Ok(_)) => false,
                    _ => true,
                }
            } else {
                false
            }
        } else {
            false
        }
    };

    if needs_reconnect {
        info!("health_check: connection {} dropped, attempting reconnect", project_id);
        state.emit_status(project_id, ConnectionStatus::Reconnecting, None);

        let credentials = {
            let connections = state.ssh_connections.lock().await;
            if let Some(conn) = connections.get(project_id) {
                conn.credentials.clone()
            } else {
                return;
            }
        };

        let reconnect_result = connect_ssh(
            &credentials.host,
            credentials.port,
            &credentials.username,
            &credentials.auth_method,
            credentials.key_path.as_deref(),
            credentials.password.as_deref(),
        )
        .await;

        match reconnect_result {
            Ok((session, sftp)) => {
                let mut connections = state.ssh_connections.lock().await;
                if let Some(conn) = connections.get_mut(project_id) {
                    conn.session = session;
                    conn.sftp = sftp.map(Arc::new);
                    conn.status = ConnectionStatus::Connected;
                    conn.reconnect_attempts = 0;
                    state.emit_status(project_id, ConnectionStatus::Connected, None);
                    info!("health_check: reconnected {}", project_id);
                }
            }
            Err(e) => {
                warn!("health_check: reconnect failed for {}: {}", project_id, e);
                let mut connections = state.ssh_connections.lock().await;
                if let Some(conn) = connections.get_mut(project_id) {
                    conn.reconnect_attempts += 1;
                    if conn.reconnect_attempts >= 10 {
                        conn.status = ConnectionStatus::Error;
                        state.emit_status(project_id, ConnectionStatus::Error, Some(e));
                        info!("health_check: giving up on {} after 10 retries", project_id);
                    } else {
                        let delay = std::cmp::min(1 << conn.reconnect_attempts, 30);
                        info!("health_check: retrying {} in {}s (attempt {}/{})", project_id, delay, conn.reconnect_attempts, 10);
                        conn.status = ConnectionStatus::Reconnecting;
                        state.emit_status(project_id, ConnectionStatus::Reconnecting, Some(format!("Reconnecting in {}s...", delay)));
                        let state_clone = state.clone();
                        let project_id_clone = project_id.to_string();
                        let credentials_clone = credentials.clone();
                        let attempt = conn.reconnect_attempts;
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(delay as u64)).await;
                            let mut connections = state_clone.ssh_connections.lock().await;
                            if let Some(conn) = connections.get_mut(&project_id_clone) {
                                if conn.status == ConnectionStatus::Reconnecting && conn.reconnect_attempts == attempt {
                                    drop(connections);
                                    let result = connect_ssh(
                                        &credentials_clone.host,
                                        credentials_clone.port,
                                        &credentials_clone.username,
                                        &credentials_clone.auth_method,
                                        credentials_clone.key_path.as_deref(),
                                        credentials_clone.password.as_deref(),
                                    )
                                    .await;
                                    match result {
                                        Ok((session, sftp)) => {
                                            let mut connections = state_clone.ssh_connections.lock().await;
                                            if let Some(conn) = connections.get_mut(&project_id_clone) {
                                                conn.session = session;
                                                conn.sftp = sftp.map(Arc::new);
                                                conn.status = ConnectionStatus::Connected;
                                                conn.reconnect_attempts = 0;
                                                state_clone.emit_status(&project_id_clone, ConnectionStatus::Connected, None);
                                                info!("health_check: reconnected {} on retry", project_id_clone);
                                            }
                                        }
                                        Err(e) => {
                                            let mut connections = state_clone.ssh_connections.lock().await;
                                            if let Some(conn) = connections.get_mut(&project_id_clone) {
                                                conn.reconnect_attempts += 1;
                                                if conn.reconnect_attempts >= 10 {
                                                    conn.status = ConnectionStatus::Error;
                                                    state_clone.emit_status(&project_id_clone, ConnectionStatus::Error, Some(e));
                                                } else {
                                                    conn.status = ConnectionStatus::Reconnecting;
                                                    state_clone.emit_status(&project_id_clone, ConnectionStatus::Reconnecting, Some(format!("Reconnect failed, will retry...")));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
            }
        }
    }
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
        .setup(|app| {
            let state: tauri::State<Arc<AppState>> = app.state();
            *state.app_handle.lock().unwrap() = Some(app.handle().clone());

            let state_clone = state.inner().clone();
            tauri::async_runtime::spawn(async move {
                start_health_check(state_clone).await;
            });

            Ok(())
        })
        .manage(Arc::new(AppState {
            ssh_connections: Mutex::new(HashMap::new()),
            app_handle: StdMutex::new(None),
        }))
        .invoke_handler(tauri::generate_handler![
            save_projects,
            load_projects,
            check_is_git_repo,
            git_init,
            git_worktree_list,
            git_worktree_list_async,
            git_worktree_add_async,
            git_worktree_remove_async,
            git_branches_list_async,
            ssh_agent_info,
            ssh_test_connection,
            ssh_connect,
            ssh_disconnect,
            ssh_list_directory,
            ssh_check_git,
            ssh_store_password,
            ssh_get_password,
            ssh_delete_password,
            fs_read_dir,
            fs_read_file,
            fs_write_file,
            fs_stat,
            fs_mkdir,
            fs_rm,
            fs_mv,
            fs_exists,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state: Arc<AppState> = window.state::<Arc<AppState>>().inner().clone();
                let handle = window.app_handle().clone();
                let connections = state.ssh_connections.blocking_lock();
                let project_ids: Vec<String> = connections.keys().cloned().collect();
                drop(connections);

                tauri::async_runtime::spawn(async move {
                    for project_id in &project_ids {
                        info!("shutdown: disconnecting {}", project_id);
                        let connections = state.ssh_connections.lock().await;
                        if let Some(conn) = connections.get(project_id) {
                            let _ = tokio::time::timeout(
                                Duration::from_secs(3),
                                conn.session.disconnect(Disconnect::ByApplication, "", "en"),
                            ).await;
                        }
                    }
                    let _ = handle.exit(0);
                });

                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
