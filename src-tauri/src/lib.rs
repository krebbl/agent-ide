mod agents;
pub mod commands;
pub mod config;
pub mod event_bus;
pub mod lsp;
mod notification;
mod pr_info;
mod pty;
pub mod pty_client;
pub mod pty_daemon;
pub mod pty_engine;
pub mod pty_protocol;
pub mod remote_ssh;
pub mod secrets;

use git2::{BranchType, Repository};
use russh::keys::agent::client::AgentClient;
use russh::keys::{PrivateKeyWithHashAlg, PublicKey};
use russh::*;
use russh_sftp::client::SftpSession;
use tokio::io::AsyncWriteExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::{info, warn};
use open::that;

pub async fn cmd_util_open_url(url: String) -> Result<(), String> {
    that(&url).map_err(|e| e.to_string())
}

#[tauri::command]
async fn util_open_url(url: String) -> Result<(), String> {
    crate::commands::util_open_url(url).await
}

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
    pub locked: bool,
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
    async fn mkdir_p(&self, path: &str) -> Result<(), String>;
    async fn rm(&self, path: &str, recursive: bool) -> Result<(), String>;
    async fn mv(&self, from: &str, to: &str) -> Result<(), String>;
    async fn exists(&self, path: &str) -> bool;
    async fn search_files(&self, root: &str, query: &str, limit: usize) -> Result<Vec<String>, String>;
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

    async fn mkdir_p(&self, path: &str) -> Result<(), String> {
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

    async fn search_files(&self, root: &str, query: &str, limit: usize) -> Result<Vec<String>, String> {
        let root_owned = root.to_owned();
        let query_lower = query.to_lowercase();

        let results = tokio::task::spawn_blocking(move || {
            let walker = ignore::WalkBuilder::new(&root_owned)
                .git_global(true)
                .git_exclude(true)
                .git_ignore(true)
                .build();

            let mut matches = Vec::new();

            for result in walker {
                if matches.len() >= limit {
                    break;
                }

                let entry = match result {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                if entry.file_type().map_or(true, |ft| ft.is_dir()) {
                    continue;
                }

                let path = entry.path();
                let rel = path.strip_prefix(&root_owned).unwrap_or(path);
                let rel_str = rel.to_string_lossy();

                if rel_str.to_lowercase().contains(&query_lower)
                    || path.file_name().map_or(false, |n| n.to_string_lossy().to_lowercase().contains(&query_lower))
                {
                    matches.push(path.to_string_lossy().to_string());
                }
            }

            matches
        })
        .await
        .map_err(|e| format!("File search failed: {}", e))?;

        Ok(results)
    }
}

pub struct SftpFileSystem {
    pub project_id: String,
    pub state: Arc<AppState>,
}

impl SftpFileSystem {
    async fn ensure_connection(&self) -> Result<Arc<SftpSession>, String> {
        ensure_ssh_connection(&self.project_id, self.state.as_ref()).await?;

        let maybe_stale = {
            let connections = self.state.ssh_connections.lock().await;
            if let Some(conn) = connections.get(&self.project_id) {
                if conn.status != ConnectionStatus::Connected {
                    return Err("SSH connection is not connected".to_string());
                }
                if let Some(sftp) = &conn.sftp {
                    match tokio::time::timeout(Duration::from_secs(3), sftp.read_dir("/")).await {
                        Ok(Ok(_)) => return Ok(Arc::clone(sftp)),
                        _ => true,
                    }
                } else {
                    return Err("SFTP is not available for this connection".to_string());
                }
            } else {
                return Err("No SSH connection found for this project".to_string());
            }
        };

        if maybe_stale {
            check_and_reconnect(&self.project_id, self.state.as_ref()).await;
            let connections = self.state.ssh_connections.lock().await;
            if let Some(conn) = connections.get(&self.project_id) {
                if conn.status == ConnectionStatus::Connected {
                    if let Some(sftp) = &conn.sftp {
                        return Ok(Arc::clone(sftp));
                    }
                }
            }
            return Err("SSH reconnection failed".to_string());
        }

        Err("No SSH connection found for this project".to_string())
    }

    async fn get_sftp(&self) -> Result<Arc<SftpSession>, String> {
        self.ensure_connection().await
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
            let mut channel = conn.session.lock().await
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
        let mut channel = conn.session.lock().await
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

    async fn mkdir_p(&self, path: &str) -> Result<(), String> {
        let resolved = self.resolve_path(path).await?;
        let sftp = self.get_sftp().await?;
        let parts: Vec<&str> = resolved.split('/').filter(|s| !s.is_empty()).collect();
        let mut accum = String::new();
        for part in &parts {
            accum.push('/');
            accum.push_str(part);
            let _ = sftp.create_dir(&accum).await;
        }
        Ok(())
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
                sftp_remove_recursive(sftp, &resolved).await
            } else {
                sftp.remove_dir(&resolved)
                    .await
                    .map_err(|e| format!("Failed to remove directory: {}", e))
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

    async fn search_files(&self, root: &str, query: &str, limit: usize) -> Result<Vec<String>, String> {
        let resolved_root = self.resolve_path(root).await?;
        let query_lower = query.to_lowercase();

        // Try git ls-files via SSH exec first
        let git_result = self.try_git_ls_files(&resolved_root, &query_lower, limit).await;
        match git_result {
            Ok(results) => return Ok(results),
            Err(_) => {} // fall through to SFTP recursive walk
        }

        // Fallback: recursive SFTP directory walk
        let sftp = self.get_sftp().await?;
        self.search_files_sftp_recursive(sftp, &resolved_root, &query_lower, limit).await
    }
}

impl SftpFileSystem {
    async fn try_git_ls_files(&self, root: &str, query_lower: &str, limit: usize) -> Result<Vec<String>, String> {
        let connections = self.state.ssh_connections.lock().await;
        let conn = connections
            .get(&self.project_id)
            .ok_or("No SSH connection found for this project")?;

        let root_escaped = shell_escape(root);
        let cmd = format!("cd {} && git ls-files --cached --others --exclude-standard 2>/dev/null", root_escaped);

        let mut channel = conn.session.lock().await
            .channel_open_session()
            .await
            .map_err(|e| format!("Failed to open channel: {}", e))?;
        channel
            .exec(false, cmd.as_str())
            .await
            .map_err(|e| format!("Failed to execute command: {}", e))?;

        let mut stdout = String::new();
        let mut exit_status: Option<u32> = None;

        while let Some(msg) = channel.wait().await {
            match msg {
                russh::ChannelMsg::Data { data } => {
                    stdout.push_str(&String::from_utf8_lossy(&data));
                }
                russh::ChannelMsg::ExitStatus { exit_status: status } => {
                    exit_status = Some(status);
                }
                russh::ChannelMsg::Close => break,
                _ => {}
            }
            if exit_status.is_some() {
                break;
            }
        }

        match exit_status {
            Some(0) => {
                let results: Vec<String> = stdout
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty() && l.to_lowercase().contains(query_lower))
                    .map(|l| format!("{}/{}", root, l))
                    .take(limit)
                    .collect();
                Ok(results)
            }
            _ => Err("git ls-files failed".to_string()),
        }
    }

    async fn search_files_sftp_recursive(
        &self,
        sftp: Arc<SftpSession>,
        dir: &str,
        query_lower: &str,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        use std::collections::VecDeque;

        let mut results = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(dir.to_string());

        while let Some(current_dir) = queue.pop_front() {
            let entries = match sftp.read_dir(&current_dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries {
                let name = entry.file_name().clone();
                if name == "." || name == ".." || name.starts_with('.') {
                    continue;
                }

                let full_path = if current_dir.ends_with('/') {
                    format!("{}{}", current_dir, name)
                } else {
                    format!("{}/{}", current_dir, name)
                };

                if entry.metadata().is_dir() {
                    queue.push_back(full_path);
                } else {
                    if full_path.to_lowercase().contains(query_lower) {
                        results.push(full_path);
                        if results.len() >= limit {
                            return Ok(results);
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}

async fn sftp_remove_recursive(sftp: Arc<SftpSession>, path: &str) -> Result<(), String> {
    let entries = sftp
        .read_dir(path)
        .await
        .map_err(|e| format!("Failed to read directory {}: {}", path, e))?;
    for entry in entries {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let child = if path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", path, name)
        };
        if entry.metadata().is_dir() {
            Box::pin(sftp_remove_recursive(Arc::clone(&sftp), &child)).await?;
        } else {
            sftp.remove_file(&child)
                .await
                .map_err(|e| format!("Failed to remove file {}: {}", child, e))?;
        }
    }
    sftp.remove_dir(path)
        .await
        .map_err(|e| format!("Failed to remove directory {}: {}", path, e))
}

async fn get_fs_provider(project_id: &str, state: &AppState) -> Result<Box<dyn FileSystemProvider>, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects.iter().find(|p| p.id == project_id).ok_or("Project not found")?;

    match &project.connection {
        Connection::Local { .. } => Ok(Box::new(LocalFileSystem)),
        Connection::Ssh { .. } => Ok(Box::new(SftpFileSystem {
            project_id: project_id.to_string(),
            state: Arc::new(state.clone()),
        })),
    }
}

pub async fn cmd_fs_read_dir(
    state: &AppState,
    project_id: String,
    path: String,
) -> Result<Vec<DirEntry>, String> {
    let provider = get_fs_provider(&project_id, state).await?;
    provider.read_dir(&path).await
}

#[tauri::command]
async fn fs_read_dir(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<DirEntry>, String> {
    crate::commands::fs_read_dir(state.inner().as_ref(), project_id, path).await
}

pub async fn cmd_fs_read_file(
    state: &AppState,
    project_id: String,
    path: String,
) -> Result<String, String> {
    let provider = get_fs_provider(&project_id, state).await?;
    provider.read_file(&path).await
}

#[tauri::command]
async fn fs_read_file(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    crate::commands::fs_read_file(state.inner().as_ref(), project_id, path).await
}

pub async fn cmd_fs_write_file(
    state: &AppState,
    project_id: String,
    path: String,
    content: String,
) -> Result<(), String> {
    let provider = get_fs_provider(&project_id, state).await?;
    provider.write_file(&path, &content).await
}

#[tauri::command]
async fn fs_write_file(
    project_id: String,
    path: String,
    content: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::fs_write_file(state.inner().as_ref(), project_id, path, content).await
}

pub async fn cmd_fs_stat(
    state: &AppState,
    project_id: String,
    path: String,
) -> Result<FileStat, String> {
    let provider = get_fs_provider(&project_id, state).await?;
    provider.stat(&path).await
}

#[tauri::command]
async fn fs_stat(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<FileStat, String> {
    crate::commands::fs_stat(state.inner().as_ref(), project_id, path).await
}

pub async fn cmd_fs_mkdir(
    state: &AppState,
    project_id: String,
    path: String,
) -> Result<(), String> {
    let provider = get_fs_provider(&project_id, state).await?;
    provider.mkdir(&path).await
}

#[tauri::command]
async fn fs_mkdir(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::fs_mkdir(state.inner().as_ref(), project_id, path).await
}

pub async fn cmd_fs_rm(
    state: &AppState,
    project_id: String,
    path: String,
    recursive: Option<bool>,
) -> Result<(), String> {
    let provider = get_fs_provider(&project_id, state).await?;
    provider.rm(&path, recursive.unwrap_or(false)).await
}

#[tauri::command]
async fn fs_rm(
    project_id: String,
    path: String,
    recursive: Option<bool>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::fs_rm(state.inner().as_ref(), project_id, path, recursive).await
}

pub async fn cmd_fs_mv(
    state: &AppState,
    project_id: String,
    from: String,
    to: String,
) -> Result<(), String> {
    let provider = get_fs_provider(&project_id, state).await?;
    provider.mv(&from, &to).await
}

#[tauri::command]
async fn fs_mv(
    project_id: String,
    from: String,
    to: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::fs_mv(state.inner().as_ref(), project_id, from, to).await
}

pub async fn cmd_fs_exists(
    state: &AppState,
    project_id: String,
    path: String,
) -> Result<bool, String> {
    let provider = get_fs_provider(&project_id, state).await?;
    Ok(provider.exists(&path).await)
}

#[tauri::command]
async fn fs_exists(
    project_id: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    crate::commands::fs_exists(state.inner().as_ref(), project_id, path).await
}

pub async fn cmd_fs_search_files(
    state: &AppState,
    project_id: String,
    root: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    let provider = get_fs_provider(&project_id, state).await?;
    provider.search_files(&root, &query, limit.unwrap_or(100)).await
}

#[tauri::command]
async fn fs_search_files(
    project_id: String,
    root: String,
    query: String,
    limit: Option<usize>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<String>, String> {
    crate::commands::fs_search_files(state.inner().as_ref(), project_id, root, query, limit).await
}

pub async fn cmd_check_agent_ready(id: String) -> Result<agents::AgentStatus, String> {
    agents::check_agent_ready(&id)
        .ok_or_else(|| format!("Unknown agent: {}", id))
}

#[tauri::command]
async fn check_agent_ready(id: String) -> Result<agents::AgentStatus, String> {
    crate::commands::check_agent_ready(id).await
}

pub async fn cmd_check_agents_ready() -> Result<Vec<agents::AgentStatus>, String> {
    Ok(agents::check_all_agents_ready())
}

#[tauri::command]
async fn check_agents_ready() -> Result<Vec<agents::AgentStatus>, String> {
    crate::commands::check_agents_ready().await
}

use remote_ssh::ClientHandler;

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
    pub session: remote_ssh::SessionHandle,
    pub sftp: Option<Arc<SftpSession>>,
    pub credentials: SshCredentials,
    pub status: ConnectionStatus,
    pub reconnect_attempts: u32,
}

#[derive(Clone)]
pub struct AppState {
    pub ssh_connections: Arc<tokio::sync::Mutex<HashMap<String, SshConnection>>>,
    pub lsp_manager: Arc<lsp::LspManager>,
    pub event_bus: crate::event_bus::EventBus,
    pub active_pty_id: Arc<parking_lot::Mutex<Option<String>>>,
    pub pty_titles: Arc<parking_lot::Mutex<HashMap<String, String>>>,
    pub pty_client: Arc<std::sync::OnceLock<Arc<crate::pty_client::PtyClient>>>,
}

impl AppState {
    pub fn new(event_bus: crate::event_bus::EventBus, lsp_manager: Arc<lsp::LspManager>) -> Arc<Self> {
        Arc::new(Self {
            ssh_connections: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            lsp_manager,
            event_bus,
            active_pty_id: Arc::new(parking_lot::Mutex::new(None)),
            pty_titles: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            pty_client: Arc::new(std::sync::OnceLock::new()),
        })
    }
}

impl AppState {
    pub fn set_active_pty(&self, pty_id: Option<String>) {
        let mut active = self.active_pty_id.lock();
        *active = pty_id.clone();
        tracing::debug!(active_pty_id = ?pty_id, "set active pty");
    }

    pub fn set_pty_title(&self, pty_id: &str, title: &str) {
        self.pty_titles.lock().insert(pty_id.to_string(), title.to_string());
    }

    pub fn clear_pty_state(&self, pty_id: &str) {
        self.pty_titles.lock().remove(pty_id);
    }

    pub fn emit_idle(&self, pty_id: &str) {
        let title = self
            .pty_titles
            .lock()
            .get(pty_id)
            .cloned()
            .unwrap_or_else(|| "Terminal".to_string());
        self.event_bus.emit(
            "pty_idle",
            pty::PtyIdleEvent {
                session_id: pty_id.to_string(),
                title,
            },
        );
    }

    pub fn emit_busy(&self, pty_id: &str) {
        let title = self
            .pty_titles
            .lock()
            .get(pty_id)
            .cloned()
            .unwrap_or_else(|| "Terminal".to_string());
        self.event_bus.emit(
            "pty_busy",
            pty::PtyBusyEvent {
                session_id: pty_id.to_string(),
                title,
            },
        );
    }

    pub fn emit_status(&self, project_id: &str, status: ConnectionStatus, error: Option<String>) {
        self.event_bus.emit("ssh_connection_status", ConnectionStatusEvent {
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

pub async fn cmd_save_projects(_state: &AppState, projects: Vec<Project>) -> Result<(), String> {
    let config_path = crate::config::app_config_dir()?;
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
async fn save_projects(
    projects: Vec<Project>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::save_projects(state.inner().as_ref(), projects).await
}

pub async fn cmd_save_expanded_projects(
    _state: &AppState,
    ids: Vec<String>,
) -> Result<(), String> {
    let config_path = crate::config::app_config_dir()?;
    std::fs::create_dir_all(&config_path)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;
    let file_path = config_path.join("expanded_projects.json");
    let json = serde_json::to_string_pretty(&ids)
        .map_err(|e| format!("Failed to serialize expanded projects: {}", e))?;
    std::fs::write(&file_path, json)
        .map_err(|e| format!("Failed to write expanded projects file: {}", e))?;
    Ok(())
}

#[tauri::command]
async fn save_expanded_projects(
    ids: Vec<String>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::save_expanded_projects(state.inner().as_ref(), ids).await
}

pub async fn cmd_load_expanded_projects(_state: &AppState) -> Result<Vec<String>, String> {
    let config_path = crate::config::app_config_dir()?;
    let file_path = config_path.join("expanded_projects.json");
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read expanded projects file: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse expanded projects file: {}", e))
}

#[tauri::command]
async fn load_expanded_projects(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<String>, String> {
    crate::commands::load_expanded_projects(state.inner().as_ref()).await
}

pub async fn cmd_load_projects(_state: &AppState) -> Result<Vec<Project>, String> {
    let config_path = crate::config::app_config_dir()?;
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
async fn load_projects(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<Project>, String> {
    crate::commands::load_projects(state.inner().as_ref()).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeTabs {
    pub open_paths: Vec<String>,
    pub active_path: Option<String>,
}

pub async fn cmd_save_editor_tabs(
    _state: &AppState,
    tabs: HashMap<String, WorktreeTabs>,
) -> Result<(), String> {
    let config_path = crate::config::app_config_dir()?;
    std::fs::create_dir_all(&config_path)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;
    let file_path = config_path.join("editor_tabs.json");
    let json = serde_json::to_string_pretty(&tabs)
        .map_err(|e| format!("Failed to serialize editor tabs: {}", e))?;
    std::fs::write(&file_path, json)
        .map_err(|e| format!("Failed to write editor tabs file: {}", e))?;
    Ok(())
}

#[tauri::command]
async fn save_editor_tabs(
    tabs: HashMap<String, WorktreeTabs>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::save_editor_tabs(state.inner().as_ref(), tabs).await
}

pub async fn cmd_load_editor_tabs(
    _state: &AppState,
) -> Result<HashMap<String, WorktreeTabs>, String> {
    let config_path = crate::config::app_config_dir()?;
    let file_path = config_path.join("editor_tabs.json");
    if !file_path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read editor tabs file: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse editor tabs file: {}", e))
}

#[tauri::command]
async fn load_editor_tabs(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<HashMap<String, WorktreeTabs>, String> {
    crate::commands::load_editor_tabs(state.inner().as_ref()).await
}

pub async fn cmd_check_is_git_repo(path: String) -> Result<bool, String> {
    let git_path = Path::new(&path).join(".git");
    Ok(git_path.exists())
}

#[tauri::command]
async fn check_is_git_repo(path: String) -> Result<bool, String> {
    crate::commands::check_is_git_repo(path).await
}

pub async fn cmd_git_init(path: String) -> Result<(), String> {
    Repository::init(&path).map_err(|e| format!("Failed to initialize git repository: {}", e))?;
    Ok(())
}

#[tauri::command]
async fn git_init(path: String) -> Result<(), String> {
    crate::commands::git_init(path).await
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

/// Shell-escape a string: wrap in single quotes, escape any embedded single quotes
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
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
    fn worktree_branch_and_status(repo: &Repository) -> (String, String) {
        let branch = if let Ok(head) = repo.head() {
            if head.is_branch() {
                head.shorthand().unwrap_or("main").to_string()
            } else {
                "main".to_string()
            }
        } else {
            "main".to_string()
        };
        let status = if is_worktree_dirty(repo) {
            "dirty".to_string()
        } else {
            "clean".to_string()
        };
        (branch, status)
    }

    let repo = Repository::open(repo_path).map_err(|e| format!("Failed to open repository: {}", e))?;

    let repo_path = Path::new(repo_path);
    let repo_path_canon = repo_path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize repository path: {}", e))?;

    let common_dir_output = std::process::Command::new("git")
        .args(["-C", repo_path.to_str().unwrap_or(""), "rev-parse", "--git-common-dir"])
        .output()
        .map_err(|e| format!("Failed to run git rev-parse: {}", e))?;
    if !common_dir_output.status.success() {
        return Err("Failed to resolve git common directory".to_string());
    }
    let common_dir_str = String::from_utf8_lossy(&common_dir_output.stdout)
        .trim()
        .to_string();
    let common_dir = PathBuf::from(&common_dir_str);
    let common_dir = if common_dir.is_absolute() {
        common_dir
    } else {
        repo_path.join(common_dir)
    };
    let main_path = common_dir
        .parent()
        .ok_or_else(|| "Invalid repository: missing common directory".to_string())?;
    let main_path = main_path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize main worktree path: {}", e))?;

    let is_current_main = repo_path_canon == main_path;
    let main_repo_owned = if is_current_main {
        None
    } else {
        Some(Repository::open(&main_path).map_err(|e| {
            format!("Failed to open main worktree repository: {}", e)
        })?)
    };
    let main_repo = main_repo_owned.as_ref().unwrap_or(&repo);

    let mut result = Vec::new();

    let (main_branch, main_status) = worktree_branch_and_status(main_repo);
    let (main_ahead, main_behind) = compute_ahead_behind(main_repo, &main_branch);
    result.push(WorktreeInfo {
        id: main_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("main")
            .to_string(),
        branch: main_branch,
        path: main_path.to_str().unwrap_or("").to_string(),
        is_main: true,
        status: main_status,
        ahead: main_ahead,
        behind: main_behind,
        locked: false,
    });

    let worktrees = repo.worktrees().map_err(|e| format!("Failed to list worktrees: {}", e))?;

    for wt_name_opt in worktrees.iter() {
        let wt_name = wt_name_opt.ok_or("Failed to read worktree name")?;

        let wt = repo.find_worktree(wt_name).map_err(|e| format!("Failed to find worktree: {}", e))?;

        let wt_path = wt.path().to_path_buf();
        let wt_path_canon = wt_path.canonicalize().unwrap_or_else(|_| wt_path.clone());
        let is_main = wt_path_canon == main_path;

        let (branch, status) = if let Ok(wt_repo) = Repository::open(&wt_path) {
            worktree_branch_and_status(&wt_repo)
        } else {
            (wt_name.to_string(), "clean".to_string())
        };

        let (ahead, behind) = compute_ahead_behind(&repo, &branch);

        result.push(WorktreeInfo {
            id: wt_name.to_string(),
            branch,
            path: wt_path.to_str().unwrap_or("").to_string(),
            is_main,
            status,
            ahead,
            behind,
            locked: matches!(wt.is_locked(), Ok(git2::WorktreeLockStatus::Locked(_))),
        });
    }

    deduplicate_worktree_ids(&mut result);
    Ok(result)
}

fn deduplicate_worktree_ids(worktrees: &mut Vec<WorktreeInfo>) {
    let mut used: HashSet<String> = HashSet::new();
    for wt in worktrees.iter_mut() {
        let original = wt.id.clone();
        if used.insert(original.clone()) {
            continue;
        }
        let mut n = 2u32;
        loop {
            let candidate = format!("{}-{}", original, n);
            if used.insert(candidate.clone()) {
                wt.id = candidate;
                break;
            }
            n += 1;
            if n > 10_000 {
                wt.id = format!("{}-{}", original, uuid::Uuid::new_v4());
                used.insert(wt.id.clone());
                break;
            }
        }
    }
}

#[cfg(test)]
mod worktree_id_tests {
    use super::*;

    fn wt(id: &str) -> WorktreeInfo {
        WorktreeInfo {
            id: id.to_string(),
            branch: "main".to_string(),
            path: format!("/tmp/{}", id),
            is_main: false,
            status: "clean".to_string(),
            ahead: 0,
            behind: 0,
            locked: false,
        }
    }

    #[test]
    fn leaves_unique_ids_unchanged() {
        let mut worktrees = vec![wt("a"), wt("b"), wt("c")];
        deduplicate_worktree_ids(&mut worktrees);
        assert_eq!(worktrees[0].id, "a");
        assert_eq!(worktrees[1].id, "b");
        assert_eq!(worktrees[2].id, "c");
    }

    #[test]
    fn appends_suffix_to_duplicate_ids() {
        let mut worktrees = vec![wt("a"), wt("a"), wt("a")];
        deduplicate_worktree_ids(&mut worktrees);
        assert_eq!(worktrees[0].id, "a");
        assert_eq!(worktrees[1].id, "a-2");
        assert_eq!(worktrees[2].id, "a-3");
    }

    #[test]
    fn avoids_collision_with_existing_suffixed_id() {
        let mut worktrees = vec![wt("a"), wt("a-2"), wt("a")];
        deduplicate_worktree_ids(&mut worktrees);
        assert_eq!(worktrees[0].id, "a");
        assert_eq!(worktrees[1].id, "a-2");
        assert_eq!(worktrees[2].id, "a-3");
    }

    #[test]
    fn deduplicates_main_and_linked_worktree_with_same_folder_name() {
        let base = std::env::temp_dir().join(format!(
            "agent-ide-care-center-test-{}",
            uuid::Uuid::new_v4()
        ));
        let main_path = base.join("care-center");
        let wt_path = base.join("wt").join("care-center");
        std::fs::create_dir_all(&main_path).unwrap();

        fn run(dir: &std::path::Path, args: &[&str]) {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .expect("git command failed");
            assert!(status.success(), "git {:?} failed", args);
        }

        run(&base, &["init", "care-center"]);
        std::fs::write(main_path.join("file.txt"), "hello").unwrap();
        run(&main_path, &["add", "file.txt"]);
        run(&main_path, &["commit", "-m", "init"]);
        run(&main_path, &["worktree", "add", wt_path.to_str().unwrap(), "-b", "feature"]);

        let worktrees = list_worktrees_local(main_path.to_str().unwrap()).unwrap();
        let ids: std::collections::HashSet<_> = worktrees.iter().map(|w| w.id.clone()).collect();
        assert_eq!(ids.len(), worktrees.len(), "worktree ids must be unique");
        assert!(ids.contains("care-center"));
        assert!(ids.contains("care-center-2"));

        let _ = std::fs::remove_dir_all(&base);
    }
}

fn compute_worktree_path(repo_path: &str, name: &str) -> Result<String, String> {
    let repo_path = repo_path.trim_end_matches('/');
    let repo_dirname = repo_path
        .split('/')
        .last()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Invalid repository path".to_string())?;
    let parent = repo_path
        .rsplitn(2, '/')
        .nth(1)
        .ok_or_else(|| "Invalid repository path: no parent directory".to_string())?;
    Ok(format!("{}/worktrees/{}/{}", parent, repo_dirname, name))
}

fn add_worktree_local(
    repo_path: &str,
    branch: &str,
    name: &str,
    new_branch: bool,
) -> Result<(), String> {
    let worktree_path = compute_worktree_path(repo_path, name)?;

    if Path::new(&worktree_path).exists() {
        return Err(format!("Path '{}' already exists", worktree_path));
    }

    if let Some(parent) = Path::new(&worktree_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create worktree parent directory: {}", e))?;
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
        vec!["worktree", "add", &worktree_path, "-b", branch]
    } else {
        vec!["worktree", "add", &worktree_path, branch]
    };
    run_git_command(repo_path, &args)?;

    Ok(())
}

/// Resolve the branch for a given worktree path by parsing
/// `git worktree list --porcelain` output.
fn resolve_worktree_branch_local(repo_path: &str, worktree_path: &str) -> Result<Option<String>, String> {
    let output = run_git_command(repo_path, &["worktree", "list", "--porcelain"])?;
    let mut current_worktree: Option<&str> = None;
    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_worktree = Some(path);
        } else if let Some(branch_ref) = line.strip_prefix("branch ") {
            if let Some(wt) = current_worktree {
                if wt == worktree_path {
                    let branch = branch_ref.trim_start_matches("refs/heads/").to_string();
                    return Ok(Some(branch));
                }
            }
            current_worktree = None;
        }
    }
    Ok(None)
}

fn remove_worktree_local(repo_path: &str, worktree_path: &str, force: bool, delete_branch: bool) -> Result<(), String> {
    let branch_to_delete = if delete_branch {
        resolve_worktree_branch_local(repo_path, worktree_path)?
    } else {
        None
    };

    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(worktree_path);

    if let Err(e) = run_git_command(repo_path, &args) {
        if force && e.contains("locked") {
            run_git_command(repo_path, &["worktree", "remove", "--force", "--force", worktree_path])?;
        } else {
            return Err(e);
        }
    }

    if let Some(branch_name) = branch_to_delete {
        run_git_command(repo_path, &["branch", "-D", &branch_name])?;
    }

    Ok(())
}

fn list_branches_local(repo_path: &str) -> Result<Vec<BranchInfo>, String> {
    let repo = Repository::open(repo_path).map_err(|e| format!("Failed to open repository: {}", e))?;

    let mut branches = Vec::new();

    let branches_iter = repo.branches(None).map_err(|e| format!("Failed to list branches: {}", e))?;
    for branch_result in branches_iter {
        let (branch, bt) = branch_result.map_err(|e| format!("Failed to read branch: {}", e))?;
        if let Some(name) = branch.name().map_err(|e| format!("Failed to get branch name: {}", e))? {
            if name != "origin/HEAD" {
                branches.push(BranchInfo {
                    name: name.to_string(),
                    is_remote: bt == BranchType::Remote,
                });
            }
        }
    }

    Ok(branches)
}

async fn run_git_command_ssh(
    project_id: &str,
    worktree_path: &str,
    args: &[&str],
    state: &AppState,
) -> Result<String, String> {
    ensure_ssh_connection(project_id, state).await?;
    let connections = state.ssh_connections.lock().await;
    let conn = connections
        .get(project_id)
        .ok_or("No SSH connection found for this project")?;

    let worktree_quoted = shlex::try_quote(worktree_path)
        .map_err(|_| "Repository path contains invalid characters".to_string())?
        .into_owned();
    let args_quoted = args
        .iter()
        .map(|a| {
            shlex::try_quote(a)
                .map_err(|_| format!("Git argument contains invalid characters: {}", a))
                .map(|q| q.into_owned())
        })
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    let cmd = format!("cd {} && git {}", worktree_quoted, args_quoted);

    info!("run_git_command_ssh: executing '{}'", cmd);

    let mut channel = conn.session.lock().await
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
    state: &AppState,
) -> Result<Vec<WorktreeInfo>, String> {
    ensure_ssh_connection(project_id, state).await?;
    let connections = state.ssh_connections.lock().await;
    let conn = connections
        .get(project_id)
        .ok_or("No SSH connection found for this project")?;

    let worktree_quoted = shlex::try_quote(repo_path)
        .map_err(|_| "Repository path contains invalid characters".to_string())?
        .into_owned();

    let script = format!(
        r#"cd {} && \
echo 'WT_LIST_BEGIN' && \
git worktree list --porcelain && \
echo 'WT_LIST_END' && \
wt_list=$(git worktree list --porcelain) && \
echo 'WT_STATES_BEGIN' && \
wt_path="" && \
wt_branch="" && \
while IFS= read -r line; do \
  case "$line" in \
    worktree*) wt_path="${{line#worktree }}" ;; \
    branch*) \
      branch="${{line#branch }}" && \
      branch="${{branch#refs/heads/}}" && \
      branch="${{branch#refs/remotes/}}" && \
      wt_branch="$branch" && \
      if [ -n "$wt_path" ]; then \
        echo "WT_STATE_BEGIN $wt_path $wt_branch" && \
        (cd "$wt_path" && if [ -n "$(git status --porcelain)" ]; then echo "WT_DIRTY"; else echo "WT_CLEAN"; fi && echo "WT_AHEAD_BEHIND" && (git rev-list --left-right --count "$wt_branch...origin/$wt_branch" 2>/dev/null || echo "0 0")) && \
        echo "WT_STATE_END"; \
      fi \
      ;; \
  esac; \
done <<EOF
$wt_list
EOF
echo 'WT_STATES_END'"#,
        worktree_quoted
    );

    info!("list_worktrees_ssh: executing combined worktree script");

    let mut channel = conn
        .session
        .lock()
        .await
        .channel_open_session()
        .await
        .map_err(|e| format!("Failed to open SSH channel: {}", e))?;

    channel
        .exec(false, script.as_str())
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
            russh::ChannelMsg::ExitStatus { exit_status: status } => {
                exit_status = Some(status);
                break;
            }
            russh::ChannelMsg::Close => break,
            _ => {}
        }
    }

    if exit_status != Some(0) {
        return Err(format!(
            "Failed to list worktrees: {}",
            stderr.trim()
        ));
    }

    let mut worktrees = Vec::new();
    let mut current: Option<WorktreeInfo> = None;
    let mut in_list = false;
    let mut in_states = false;
    let mut state_idx: Option<usize> = None;

    for line in stdout.lines() {
        let line = line.trim();

        if line == "WT_LIST_BEGIN" {
            in_list = true;
            in_states = false;
            continue;
        }
        if line == "WT_LIST_END" {
            in_list = false;
            if let Some(wt) = current.take() {
                worktrees.push(wt);
            }
            continue;
        }
        if line == "WT_STATES_BEGIN" {
            in_states = true;
            continue;
        }
        if line == "WT_STATES_END" {
            break;
        }

        if in_list {
            if line.starts_with("worktree ") {
                if let Some(wt) = current.take() {
                    worktrees.push(wt);
                }
                let path = line.trim_start_matches("worktree ").to_string();
                let id = path.clone();
                current = Some(WorktreeInfo {
                    id,
                    branch: String::new(),
                    path,
                    is_main: false,
                    status: "unknown".to_string(),
                    ahead: 0,
                    behind: 0,
                    locked: false,
                });
            } else if line.starts_with("branch ") {
                if let Some(ref mut wt) = current {
                    let branch_ref = line.trim_start_matches("branch ");
                    wt.branch = branch_ref
                        .trim_start_matches("refs/heads/")
                        .trim_start_matches("refs/remotes/")
                        .to_string();
                }
            } else if line == "bare" {
                if let Some(ref mut wt) = current {
                    wt.is_main = true;
                }
            } else if line.starts_with("locked") {
                if let Some(ref mut wt) = current {
                    wt.locked = true;
                }
            } else if line.starts_with("HEAD ") {
                if let Some(ref mut wt) = current {
                    let head_ref = line.trim_start_matches("HEAD ");
                    if wt.branch.is_empty() {
                        wt.branch = head_ref
                            .trim_start_matches("refs/heads/")
                            .trim_start_matches("refs/remotes/")
                            .to_string();
                    }
                }
            }
        } else if in_states {
            if let Some(rest) = line.strip_prefix("WT_STATE_BEGIN ") {
                let mut parts = rest.rsplitn(2, ' ');
                let _branch = parts.next();
                let path = parts.next().unwrap_or("").to_string();
                state_idx = worktrees.iter().position(|w| w.path == path);
            } else if line == "WT_DIRTY" {
                if let Some(idx) = state_idx {
                    worktrees[idx].status = "dirty".to_string();
                }
            } else if line == "WT_CLEAN" {
                if let Some(idx) = state_idx {
                    worktrees[idx].status = "clean".to_string();
                }
            } else if line == "WT_STATE_END" {
                state_idx = None;
            } else if !line.is_empty() {
                if let Some(idx) = state_idx {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() == 2 {
                        if let (Ok(ahead), Ok(behind)) =
                            (parts[0].parse::<i32>(), parts[1].parse::<i32>())
                        {
                            worktrees[idx].ahead = ahead;
                            worktrees[idx].behind = behind;
                        }
                    }
                }
            }
        }
    }

    if !worktrees.is_empty() {
        worktrees[0].is_main = true;
    }

    deduplicate_worktree_ids(&mut worktrees);
    Ok(worktrees)
}

async fn add_worktree_ssh(
    project_id: &str,
    repo_path: &str,
    branch: &str,
    name: &str,
    new_branch: bool,
    state: &AppState,
) -> Result<(), String> {
    let worktree_path = compute_worktree_path(repo_path, name)?;

    let fs = get_fs_provider(project_id, state).await?;
    if fs.exists(&worktree_path).await {
        return Err(format!("Path '{}' already exists", worktree_path));
    }

    if let Some(parent) = worktree_path.rsplitn(2, '/').nth(1) {
        fs.mkdir_p(parent).await?;
    }

    if new_branch {
        run_git_command_ssh(project_id, repo_path, &["worktree", "add", &worktree_path, "-b", branch], state).await?;
    } else {
        run_git_command_ssh(project_id, repo_path, &["worktree", "add", &worktree_path, branch], state).await?;
    }

    Ok(())
}

async fn resolve_worktree_branch_ssh(
    project_id: &str,
    repo_path: &str,
    worktree_path: &str,
    state: &AppState,
) -> Result<Option<String>, String> {
    let output = run_git_command_ssh(project_id, repo_path, &["worktree", "list", "--porcelain"], state).await?;
    let mut current_worktree: Option<&str> = None;
    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_worktree = Some(path);
        } else if let Some(branch_ref) = line.strip_prefix("branch ") {
            if let Some(wt) = current_worktree {
                if wt == worktree_path {
                    let branch = branch_ref.trim_start_matches("refs/heads/").to_string();
                    return Ok(Some(branch));
                }
            }
            current_worktree = None;
        }
    }
    Ok(None)
}

async fn remove_worktree_ssh(
    project_id: &str,
    repo_path: &str,
    worktree_path: &str,
    force: bool,
    state: &AppState,
    delete_branch: bool,
) -> Result<(), String> {
    let branch_to_delete = if delete_branch {
        resolve_worktree_branch_ssh(project_id, repo_path, worktree_path, state).await?
    } else {
        None
    };

    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(worktree_path);

    if let Err(e) = run_git_command_ssh(project_id, repo_path, &args, state).await {
        if force && e.contains("locked") {
            run_git_command_ssh(
                project_id,
                repo_path,
                &["worktree", "remove", "--force", "--force", worktree_path],
                state,
            )
            .await?;
        } else {
            return Err(e);
        }
    }

    if let Some(branch_name) = branch_to_delete {
        run_git_command_ssh(project_id, repo_path, &["branch", "-D", &branch_name], state).await?;
    }

    Ok(())
}

async fn list_branches_ssh(
    project_id: &str,
    repo_path: &str,
    state: &AppState,
) -> Result<Vec<BranchInfo>, String> {
    info!("list_branches_ssh: project_id={} repo_path={}", project_id, repo_path);
    let mut branches = Vec::new();

    let local_output = run_git_command_ssh(
        project_id,
        repo_path,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        state,
    )
    .await
    .map_err(|e| {
        warn!("list_branches_ssh: local refs failed: {}", e);
        format!("Failed to list local branches: {}", e)
    })?;
    info!("list_branches_ssh: local output='{}'", local_output);
    for line in local_output.lines() {
        let name = line.trim();
        if !name.is_empty() {
            branches.push(BranchInfo {
                name: name.to_string(),
                is_remote: false,
            });
        }
    }

    let remote_output = run_git_command_ssh(
        project_id,
        repo_path,
        &["for-each-ref", "--format=%(refname:short)", "refs/remotes/"],
        state,
    )
    .await
    .map_err(|e| {
        warn!("list_branches_ssh: remote refs failed: {}", e);
        format!("Failed to list remote branches: {}", e)
    })?;
    info!("list_branches_ssh: remote output='{}'", remote_output);
    for line in remote_output.lines() {
        let name = line.trim();
        if !name.is_empty() && name != "origin/HEAD" {
            branches.push(BranchInfo {
                name: name.to_string(),
                is_remote: true,
            });
        }
    }

    info!("list_branches_ssh: returning {} branches", branches.len());
    Ok(branches)
}

pub async fn cmd_git_worktree_list(
    state: &AppState,
    project_id: String,
) -> Result<Vec<WorktreeInfo>, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    match &project.connection {
        Connection::Local { path } => list_worktrees_local(path),
        Connection::Ssh { .. } => Err("SSH worktree listing requires async execution. Use git_worktree_list_async instead.".to_string()),
    }
}

#[tauri::command]
async fn git_worktree_list(
    project_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<WorktreeInfo>, String> {
    crate::commands::git_worktree_list(state.inner().as_ref(), project_id).await
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

pub async fn cmd_git_worktree_list_async(
    state: &AppState,
    project_id: String,
) -> Result<Vec<WorktreeInfo>, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    match &project.connection {
        Connection::Local { path } => list_worktrees_local(path),
        Connection::Ssh { .. } => {
            let repo_path = get_repo_path(project);
            list_worktrees_ssh(&project_id, &repo_path, state).await
        }
    }
}

#[tauri::command]
async fn git_worktree_list_async(
    project_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<WorktreeInfo>, String> {
    crate::commands::git_worktree_list_async(state.inner().as_ref(), project_id).await
}

pub async fn cmd_git_worktree_add_async(
    state: &AppState,
    project_id: String,
    branch: String,
    name: String,
    new_branch: Option<bool>,
) -> Result<(), String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    let new_branch = new_branch.unwrap_or(false);

    match &project.connection {
        Connection::Local { path: repo_path } => add_worktree_local(repo_path, &branch, &name, new_branch),
        Connection::Ssh { .. } => {
            let repo_path = get_repo_path(project);
            add_worktree_ssh(&project_id, &repo_path, &branch, &name, new_branch, state).await
        }
    }
}

#[tauri::command]
async fn git_worktree_add_async(
    project_id: String,
    branch: String,
    name: String,
    new_branch: Option<bool>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::git_worktree_add_async(state.inner().as_ref(), project_id, branch, name, new_branch).await
}

pub async fn cmd_git_worktree_remove_async(
    state: &AppState,
    project_id: String,
    worktree_path: String,
    force: Option<bool>,
    delete_branch: Option<bool>,
) -> Result<(), String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    let force = force.unwrap_or(false);
    let delete_branch = delete_branch.unwrap_or(false);

    match &project.connection {
        Connection::Local { path: repo_path } => {
            remove_worktree_local(repo_path, &worktree_path, force, delete_branch)
        }
        Connection::Ssh { .. } => {
            let repo_path = get_repo_path(project);
            remove_worktree_ssh(&project_id, &repo_path, &worktree_path, force, state, delete_branch).await
        }
    }
}

#[tauri::command]
async fn git_worktree_remove_async(
    project_id: String,
    worktree_path: String,
    force: Option<bool>,
    delete_branch: Option<bool>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::commands::git_worktree_remove_async(
        state.inner().as_ref(),
        project_id,
        worktree_path,
        force,
        delete_branch,
    )
    .await
}

pub async fn cmd_git_branches_list_async(
    state: &AppState,
    project_id: String,
) -> Result<Vec<BranchInfo>, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    match &project.connection {
        Connection::Local { path } => list_branches_local(path),
        Connection::Ssh { .. } => {
            let repo_path = get_repo_path(project);
            list_branches_ssh(&project_id, &repo_path, state).await
        }
    }
}

#[tauri::command]
async fn git_branches_list_async(
    project_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<BranchInfo>, String> {
    crate::commands::git_branches_list_async(state.inner().as_ref(), project_id).await
}

fn filter_available_branches(
    branches: Vec<BranchInfo>,
    assigned: &[String],
) -> Vec<BranchInfo> {
    let assigned_set: HashSet<&str> = assigned.iter().map(|s| s.as_str()).collect();
    let local_names: HashSet<String> = branches
        .iter()
        .filter(|b| !b.is_remote)
        .map(|b| b.name.clone())
        .collect();

    branches
        .into_iter()
        .filter(|b| {
            if assigned_set.contains(b.name.as_str()) {
                return false;
            }
            if b.is_remote {
                if let Some(base) = b.name.splitn(2, '/').nth(1) {
                    if assigned_set.contains(base) || local_names.iter().any(|n| n == base) {
                        return false;
                    }
                }
            }
            true
        })
        .collect()
}

fn parse_worktree_branch_names(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| {
            l.strip_prefix("branch ").map(|b| {
                b.trim_start_matches("refs/heads/")
                    .trim_start_matches("refs/remotes/")
                    .to_string()
            })
        })
        .collect()
}

fn list_worktree_branch_names_local(repo_path: &str) -> Result<Vec<String>, String> {
    let output = run_git_command(repo_path, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_branch_names(&output))
}

fn fetch_remotes_local(repo_path: &str) -> Result<(), String> {
    run_git_command(repo_path, &["fetch", "--all", "--prune"]).map(|_| ())
}

fn list_branches_available_for_worktrees_local(
    repo_path: &str,
) -> Result<Vec<BranchInfo>, String> {
    if let Err(e) = fetch_remotes_local(repo_path) {
        warn!("list_branches_available_for_worktrees_local: fetch failed: {}", e);
    }
    let branches = list_branches_local(repo_path)?;
    let assigned = list_worktree_branch_names_local(repo_path)?;
    Ok(filter_available_branches(branches, &assigned))
}

async fn list_worktree_branch_names_ssh(
    project_id: &str,
    repo_path: &str,
    state: &Arc<AppState>,
) -> Result<Vec<String>, String> {
    let output = run_git_command_ssh(project_id, repo_path, &["worktree", "list", "--porcelain"], state).await?;
    Ok(parse_worktree_branch_names(&output))
}

async fn fetch_remotes_ssh(
    project_id: &str,
    repo_path: &str,
    state: &AppState,
) -> Result<(), String> {
    run_git_command_ssh(project_id, repo_path, &["fetch", "--all", "--prune"], state)
        .await
        .map(|_| ())
}

async fn list_branches_available_for_worktrees_ssh(
    project_id: &str,
    repo_path: &str,
    state: &AppState,
) -> Result<Vec<BranchInfo>, String> {
    if let Err(e) = fetch_remotes_ssh(project_id, repo_path, state).await {
        warn!("list_branches_available_for_worktrees_ssh: fetch failed: {}", e);
    }
    let branches = list_branches_ssh(project_id, repo_path, state).await?;
    let assigned = list_worktree_branch_names_ssh(project_id, repo_path, state).await?;
    Ok(filter_available_branches(branches, &assigned))
}

pub async fn cmd_git_branches_available_for_worktrees_async(
    state: &AppState,
    project_id: String,
) -> Result<Vec<BranchInfo>, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    match &project.connection {
        Connection::Local { path } => list_branches_available_for_worktrees_local(path),
        Connection::Ssh { .. } => {
            let repo_path = get_repo_path(project);
            list_branches_available_for_worktrees_ssh(&project_id, &repo_path, state).await
        }
    }
}

#[tauri::command]
async fn git_branches_available_for_worktrees_async(
    project_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<BranchInfo>, String> {
    crate::commands::git_branches_available_for_worktrees_async(state.inner().as_ref(), project_id).await
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
    cmd_ssh_agent_info().await
}

pub async fn cmd_ssh_agent_info() -> Result<SshAgentInfo, String> {
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
    cmd_ssh_test_connection(host, port, username, auth_method, key_path, password).await
}

pub async fn cmd_ssh_test_connection(
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
    cmd_ssh_connect(state.inner().as_ref(), project_id, host, port, username, auth_method, key_path, password).await
}

pub async fn cmd_ssh_connect(
    state: &AppState,
    project_id: String,
    host: String,
    port: u16,
    username: String,
    auth_method: String,
    key_path: Option<String>,
    password: Option<String>,
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
        session: Arc::new(Mutex::new(session)),
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
    cmd_ssh_disconnect(state.inner().as_ref(), project_id).await
}

pub async fn cmd_ssh_disconnect(
    state: &AppState,
    project_id: String,
) -> Result<(), String> {
    info!("ssh_disconnect: project_id={}", project_id);
    state.lsp_manager.stop_project(&project_id).await;
    let mut connections = state.ssh_connections.lock().await;
    if let Some(conn) = connections.remove(&project_id) {
        info!("ssh_disconnect: disconnecting session");
        let _ = conn.session.lock().await.disconnect(Disconnect::ByApplication, "", "en").await;
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
    cmd_ssh_list_directory(state.inner().as_ref(), project_id, path).await
}

pub async fn cmd_ssh_list_directory(
    state: &AppState,
    project_id: String,
    path: String,
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
    cmd_ssh_check_git(state.inner().as_ref(), project_id, path).await
}

pub async fn cmd_ssh_check_git(
    state: &AppState,
    project_id: String,
    path: String,
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
async fn ssh_store_password(
    project_id: String,
    password: String,
) -> Result<(), String> {
    cmd_ssh_store_password(project_id, password).await
}

pub async fn cmd_ssh_store_password(
    project_id: String,
    password: String,
) -> Result<(), String> {
    info!("ssh_store_password: project_id={}", project_id);
    crate::secrets::set_secret(&format!("ssh-password-{}", project_id), &password)
}

#[tauri::command]
async fn ssh_get_password(
    project_id: String,
) -> Result<Option<String>, String> {
    cmd_ssh_get_password(project_id).await
}

pub async fn cmd_ssh_get_password(
    project_id: String,
) -> Result<Option<String>, String> {
    info!("ssh_get_password: project_id={}", project_id);
    crate::secrets::get_secret(&format!("ssh-password-{}", project_id))
}

#[tauri::command]
async fn ssh_delete_password(
    project_id: String,
) -> Result<(), String> {
    cmd_ssh_delete_password(project_id).await
}

pub async fn cmd_ssh_delete_password(
    project_id: String,
) -> Result<(), String> {
    info!("ssh_delete_password: project_id={}", project_id);
    crate::secrets::delete_secret(&format!("ssh-password-{}", project_id))
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

/// Guarantees `state.ssh_connections` has a live, connected session for `project_id`
/// before an SSH-backed operation (git-over-ssh, sftp) runs. Unlike the periodic
/// health check, this also covers the case where no entry exists yet at all - e.g.
/// the initial `ssh_connect` call made at app/project load never completed or was
/// silently dropped, while the terminal (which owns its own independent SSH
/// connection in the pty daemon) is still connected.
async fn ensure_ssh_connection(project_id: &str, state: &AppState) -> Result<(), String> {
    {
        let connections = state.ssh_connections.lock().await;
        if let Some(conn) = connections.get(project_id) {
            if conn.status == ConnectionStatus::Connected {
                return Ok(());
            }
        }
    }

    let existing_credentials = {
        let connections = state.ssh_connections.lock().await;
        connections.get(project_id).map(|c| c.credentials.clone())
    };

    let credentials = match existing_credentials {
        Some(creds) => creds,
        None => {
            let projects = crate::commands::load_projects(state).await?;
            let project = projects
                .iter()
                .find(|p| p.id == project_id)
                .ok_or("Project not found")?;
            match &project.connection {
                Connection::Ssh { host, port, username, auth_method, key_path, .. } => {
                    let password = secrets::get_secret(project_id).ok().flatten();
                    SshCredentials {
                        host: host.clone(),
                        port: *port,
                        username: username.clone(),
                        auth_method: auth_method.clone(),
                        key_path: key_path.clone(),
                        password,
                    }
                }
                Connection::Local { .. } => {
                    return Err("Project is not an SSH project".to_string());
                }
            }
        }
    };

    info!("ensure_ssh_connection: (re)connecting {}", project_id);
    state.emit_status(project_id, ConnectionStatus::Reconnecting, None);

    match connect_ssh(
        &credentials.host,
        credentials.port,
        &credentials.username,
        &credentials.auth_method,
        credentials.key_path.as_deref(),
        credentials.password.as_deref(),
    )
    .await
    {
        Ok((session, sftp)) => {
            let mut connections = state.ssh_connections.lock().await;
            connections.insert(
                project_id.to_string(),
                SshConnection {
                    session: Arc::new(Mutex::new(session)),
                    sftp: sftp.map(Arc::new),
                    credentials,
                    status: ConnectionStatus::Connected,
                    reconnect_attempts: 0,
                },
            );
            drop(connections);
            state.emit_status(project_id, ConnectionStatus::Connected, None);
            Ok(())
        }
        Err(e) => {
            warn!("ensure_ssh_connection: failed to connect {}: {}", project_id, e);
            state.emit_status(project_id, ConnectionStatus::Error, Some(e.clone()));
            Err(format!("Failed to establish SSH connection: {}", e))
        }
    }
}

async fn check_and_reconnect(project_id: &str, state: &AppState) {
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
                    conn.session = Arc::new(Mutex::new(session));
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
                                                conn.session = Arc::new(Mutex::new(session));
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

pub fn run_pty_daemon(daemonize: bool) -> Result<(), String> {
    let config_dir = pty_client::daemon_config_dir();
    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        eprintln!("Failed to create config directory: {}", e);
    }
    #[cfg(unix)]
    if daemonize {
        let daemonize = daemonize::Daemonize::new()
            .working_directory(config_dir);
        if let Err(e) = daemonize.start() {
            eprintln!("Failed to daemonize: {}", e);
            std::process::exit(1);
        }
    }
    let pid_path = pty_client::daemon_pid_path();
    let _ = std::fs::write(&pid_path, std::process::id().to_string());
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create tokio runtime: {}", e))?
        .block_on(async {
            let daemon = pty_daemon::PtyDaemon::new(
                pty_client::daemon_socket_path(),
                pty_client::daemon_persistence_path(),
            );
            daemon.run().await
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let context = tauri::generate_context!();
    if std::env::var("AGENT_IDE_CONFIG_DIR").is_err() {
        if let Some(dir) = dirs::config_dir() {
            let base = dir.join("agent-ide");
            let instance = context
                .config()
                .identifier
                .rsplit('.')
                .next()
                .unwrap_or("agent-ide");
            let config_dir = if instance == "agent-ide" {
                base
            } else {
                base.join(instance)
            };
            std::env::set_var("AGENT_IDE_CONFIG_DIR", config_dir);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let event_bus = crate::event_bus::EventBus::Tauri(app.handle().clone());
            let state = crate::AppState::new(event_bus.clone(), Arc::new(lsp::LspManager::default()));
            app.manage(state.clone());
            crate::notification::set_event_bus(event_bus.clone());

            let state_clone = state.clone();
            tauri::async_runtime::spawn(async move {
                start_health_check(state_clone).await;
            });

            let pty_client = Arc::new(
                tauri::async_runtime::block_on(async {
                    pty_client::PtyClient::new(
                        pty_client::daemon_socket_path(),
                        event_bus.clone(),
                        true,
                    )
                    .await
                })
                .expect("failed to connect to pty daemon"),
            );
            let _ = state.pty_client.set(pty_client);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            save_projects,
            load_projects,
            save_expanded_projects,
            load_expanded_projects,
            save_editor_tabs,
            load_editor_tabs,
            check_is_git_repo,
            git_init,
            git_worktree_list,
            git_worktree_list_async,
            git_worktree_add_async,
            git_worktree_remove_async,
            git_branches_list_async,
            git_branches_available_for_worktrees_async,
            ssh_agent_info,
            ssh_test_connection,
            ssh_connect,
            ssh_disconnect,
            ssh_list_directory,
            ssh_check_git,
            ssh_store_password,
            ssh_get_password,
            ssh_delete_password,
            notification::notification_show,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            pty::pty_set_active,
            pty::pty_list_sessions,
            pty::pty_register_ssh_project,
            util_open_url,
            fs_read_dir,
            fs_read_file,
            fs_write_file,
            fs_stat,
            fs_mkdir,
            fs_rm,
            fs_mv,
            fs_exists,
            fs_search_files,
            check_agent_ready,
            check_agents_ready,
            pr_info::pr_for_branch,
            pr_info::pr_list_for_repo,
            lsp::lsp_start,
            lsp::lsp_request,
            lsp::lsp_notify,
            lsp::lsp_stop,
            lsp::lsp_list,
            lsp::lsp_server_available,
        ])
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
            }
        })
        .build(context)
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state: tauri::State<Arc<AppState>> = app_handle.state();
                let manager = state.lsp_manager.clone();
                let _ = tauri::async_runtime::block_on(async move {
                    manager.shutdown_all().await;
                });
            }
        });
}
