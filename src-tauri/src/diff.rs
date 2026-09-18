use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;

use crate::{shell_escape, AppState, Connection};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileEntry {
    pub path: String,
    pub old_path: Option<String>,
    pub status: String,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileContent {
    pub path: String,
    pub base: Option<String>,
    pub modified: Option<String>,
    pub binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffComment {
    pub id: String,
    pub project_id: String,
    pub branch: String,
    pub file: String,
    pub side: String,
    pub line: u32,
    pub anchor_hash: String,
    pub body: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub resolved_line: Option<u32>,
    #[serde(default)]
    pub orphan: bool,
}

fn is_binary(data: &[u8]) -> bool {
    data.iter().take(8000).any(|&b| b == 0)
}

fn fnv1a64(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.trim_end_matches('\r').as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Local (git2)
// ---------------------------------------------------------------------------

fn status_label(status: git2::Delta) -> &'static str {
    match status {
        git2::Delta::Added | git2::Delta::Untracked => "added",
        git2::Delta::Deleted => "deleted",
        git2::Delta::Renamed | git2::Delta::Copied => "renamed",
        git2::Delta::Conflicted => "conflicted",
        _ => "modified",
    }
}

fn diff_summary_local(
    worktree_path: &str,
    base_branch: Option<&str>,
) -> Result<Vec<DiffFileEntry>, String> {
    let repo = git2::Repository::open(worktree_path)
        .map_err(|e| format!("Failed to open repository: {}", e))?;
    let head_tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let mut diff = match base_branch {
        // Branch mode: committed changes from the merge base with the target
        // branch up to HEAD.
        Some(base) => {
            let base_commit = repo
                .revparse_single(base)
                .and_then(|o| o.peel_to_commit())
                .map_err(|e| format!("Base branch {} not found: {}", base, e))?;
            let head_commit = repo
                .head()
                .and_then(|h| h.peel_to_commit())
                .map_err(|e| format!("Failed to resolve HEAD: {}", e))?;
            let mb = repo
                .merge_base(head_commit.id(), base_commit.id())
                .map_err(|e| format!("No merge base with {}: {}", base, e))?;
            let mb_tree = repo
                .find_commit(mb)
                .and_then(|c| c.tree())
                .map_err(|e| format!("Failed to load merge base tree: {}", e))?;
            opts.include_untracked(false);
            repo.diff_tree_to_tree(
                Some(&mb_tree),
                head_tree.as_ref(),
                Some(&mut opts),
            )
            .map_err(|e| format!("Failed to compute diff: {}", e))?
        }
        None => repo
            .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut opts))
            .map_err(|e| format!("Failed to compute diff: {}", e))?,
    };
    let mut find = git2::DiffFindOptions::new();
    find.renames(true);
    let _ = diff.find_similar(Some(&mut find));

    let mut entries = Vec::new();
    for i in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(i) else {
            continue;
        };
        let old = delta
            .old_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        let new = delta
            .new_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        let (mut insertions, mut deletions) = (0u32, 0u32);
        if let Ok(Some(patch)) = git2::Patch::from_diff(&diff, i) {
            for h in 0..patch.num_hunks() {
                if let Ok(line_count) = patch.num_lines_in_hunk(h) {
                    for l in 0..line_count {
                        if let Ok(line) = patch.line_in_hunk(h, l) {
                            match line.origin() {
                                '+' => insertions += 1,
                                '-' => deletions += 1,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        let path = new.or_else(|| old.clone()).unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        entries.push(DiffFileEntry {
            path,
            old_path: if delta.status() == git2::Delta::Renamed {
                old
            } else {
                None
            },
            status: status_label(delta.status()).to_string(),
            insertions,
            deletions,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// Hex sha of the merge base between HEAD and `base_branch`, if both exist.
fn merge_base_local(worktree_path: &str, base_branch: &str) -> Result<Option<String>, String> {
    let repo = git2::Repository::open(worktree_path)
        .map_err(|e| format!("Failed to open repository: {}", e))?;
    let head = match repo.head() {
        Ok(h) => h.peel_to_commit().map_err(|e| format!("Failed to resolve HEAD: {}", e))?,
        Err(_) => return Ok(None),
    };
    let base = match repo.revparse_single(base_branch).and_then(|o| o.peel_to_commit()) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    match repo.merge_base(head.id(), base.id()) {
        Ok(mb) => Ok(Some(mb.to_string())),
        Err(_) => Ok(None),
    }
}

/// Blob content at `rev_spec` (e.g. "HEAD" or a merge-base sha) for `rel`.
fn blob_at_local(
    worktree_path: &str,
    rev_spec: &str,
    rel: &str,
) -> Result<Option<(Vec<u8>, bool)>, String> {
    let repo = git2::Repository::open(worktree_path)
        .map_err(|e| format!("Failed to open repository: {}", e))?;
    let tree = match repo.revparse_single(rev_spec) {
        Ok(obj) => match obj.peel_to_tree() {
            Ok(t) => t,
            Err(_) => return Ok(None),
        },
        Err(_) => return Ok(None),
    };
    let entry = match tree.get_path(Path::new(rel)) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    let obj = entry
        .to_object(&repo)
        .map_err(|e| format!("Failed to load object: {}", e))?;
    let blob = obj
        .as_blob()
        .ok_or_else(|| format!("Entry {} in {} is not a file", rel, rev_spec))?;
    let data = blob.content();
    Ok(Some((data.to_vec(), is_binary(data))))
}

fn modified_content_local(worktree_path: &str, rel: &str) -> (Option<String>, bool) {
    let abs = Path::new(worktree_path).join(rel);
    match std::fs::read(&abs) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => (Some(text), false),
            Err(_) => (None, true),
        },
        Err(_) => (None, false),
    }
}

// ---------------------------------------------------------------------------
// Remote (SSH exec + SFTP)
// ---------------------------------------------------------------------------

async fn ssh_exec(
    state: &AppState,
    project_id: &str,
    cmd: &str,
) -> Result<(Option<u32>, Vec<u8>), String> {
    let connections = state.ssh_connections.lock().await;
    let conn = connections
        .get(project_id)
        .ok_or("No SSH connection found for this project")?;
    let mut channel = conn
        .session
        .lock()
        .await
        .channel_open_session()
        .await
        .map_err(|e| format!("Failed to open channel: {}", e))?;
    channel
        .exec(false, cmd)
        .await
        .map_err(|e| format!("Failed to execute command: {}", e))?;

    let mut out: Vec<u8> = Vec::new();
    let mut exit_status: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => out.extend_from_slice(&data),
            russh::ChannelMsg::ExtendedData { data, .. } => {}
            russh::ChannelMsg::ExitStatus { exit_status: s } => exit_status = Some(s),
            russh::ChannelMsg::Close => break,
            _ => {}
        }
        if exit_status.is_some() {
            break;
        }
    }
    Ok((exit_status, out))
}

/// Parse `git status --porcelain=v1 -z`: entries "XY path\0", renames append
/// the orig path as an extra NUL-terminated field.
fn parse_porcelain_v1_z(data: &[u8]) -> Vec<DiffFileEntry> {
    let text = String::from_utf8_lossy(data);
    let mut fields = text.split('\0').filter(|f| !f.is_empty());
    let mut entries = Vec::new();
    while let Some(field) = fields.next() {
        if field.len() < 4 {
            continue;
        }
        let xy: Vec<char> = field.chars().take(2).collect();
        let path: String = field.chars().skip(3).collect();
        let mut orig: Option<String> = None;
        if xy.iter().any(|c| *c == 'R' || *c == 'C') {
            orig = fields.next().map(String::from);
        }
        let status = if xy.contains(&'!') {
            continue;
        } else if xy.contains(&'?') {
            "added"
        } else if xy.contains(&'U') {
            "conflicted"
        } else if xy.contains(&'R') || xy.contains(&'C') {
            "renamed"
        } else if xy.contains(&'A') {
            "added"
        } else if xy.contains(&'D') {
            "deleted"
        } else {
            "modified"
        };
        entries.push(DiffFileEntry {
            path,
            old_path: orig,
            status: status.to_string(),
            insertions: 0,
            deletions: 0,
        });
    }
    entries.retain(|e| !e.path.is_empty());
    entries
}

/// Parse `git diff --name-status -z <mb> HEAD`: entries "S path\0", renames
/// carry a score ("R100") and append the orig path as an extra field.
fn parse_name_status_z(data: &[u8]) -> Vec<DiffFileEntry> {
    let text = String::from_utf8_lossy(data);
    let mut fields = text.split('\0').filter(|f| !f.is_empty());
    let mut entries = Vec::new();
    while let Some(code) = fields.next() {
        let Some(path) = fields.next() else {
            break;
        };
        let first = code.chars().next().unwrap_or('M');
        let mut orig: Option<String> = None;
        if first == 'R' || first == 'C' {
            orig = fields.next().map(String::from);
        }
        let status = match first {
            'A' => "added",
            'D' => "deleted",
            'R' | 'C' => "renamed",
            'U' => "conflicted",
            _ => "modified",
        };
        entries.push(DiffFileEntry {
            path: path.to_string(),
            old_path: orig,
            status: status.to_string(),
            insertions: 0,
            deletions: 0,
        });
    }
    entries.retain(|e| !e.path.is_empty());
    entries
}

/// Parse `git diff HEAD --numstat` into per-path counts keyed by the new path.
fn parse_numstat(data: &str) -> HashMap<String, (u32, u32)> {
    let mut map = HashMap::new();
    for line in data.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(ins), Some(del), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let new_path = if let Some(arrow) = path.find(" => ") {
            if let Some(open) = path[..arrow].rfind('{') {
                let close = path[arrow..].find('}').map(|i| arrow + i);
                match close {
                    Some(close) => {
                        let prefix = &path[..open];
                        let suffix = &path[close + 1..];
                        let new = &path[arrow + 4..close];
                        format!("{}{}{}", prefix, new, suffix)
                    }
                    None => path[arrow + 4..].to_string(),
                }
            } else {
                path[arrow + 4..].to_string()
            }
        } else {
            path.to_string()
        };
        let (ins, del) = match (ins.parse::<u32>(), del.parse::<u32>()) {
            (Ok(i), Ok(d)) => (i, d),
            _ => (0, 0),
        };
        map.insert(new_path, (ins, del));
    }
    map
}

async fn remote_summary(
    state: &AppState,
    project_id: &str,
    worktree_path: &str,
    base_branch: Option<&str>,
) -> Result<Vec<DiffFileEntry>, String> {
    let script = match base_branch {
        Some(base) => format!(
            "cd {wt} && mb=$(git merge-base HEAD {origin} 2>/dev/null || git merge-base HEAD {local} 2>/dev/null) && [ -n \"$mb\" ] && git -c core.quotepath=false diff --name-status -z \"$mb\" HEAD && printf '\\001' && git -c core.quotepath=false diff \"$mb\" HEAD --numstat",
            wt = shell_escape(worktree_path),
            origin = shell_escape(&format!("origin/{}", base)),
            local = shell_escape(base),
        ),
        None => format!(
            "cd {} && git -c core.quotepath=false status --porcelain=v1 -z && printf '\\001' && git -c core.quotepath=false diff HEAD --numstat",
            shell_escape(worktree_path)
        ),
    };
    let (exit, out) = ssh_exec(state, project_id, &script).await?;
    if exit != Some(0) {
        return Err("git status/diff failed on remote host".to_string());
    }
    let sep = out.iter().position(|&b| b == 1).ok_or("Unexpected git output")?;
    let mut entries = match base_branch {
        Some(_) => parse_name_status_z(&out[..sep]),
        None => parse_porcelain_v1_z(&out[..sep]),
    };
    let numstat = String::from_utf8_lossy(&out[sep + 1..]);
    let counts = parse_numstat(&numstat);
    for entry in entries.iter_mut() {
        if let Some((ins, del)) = counts.get(&entry.path) {
            entry.insertions = *ins;
            entry.deletions = *del;
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

async fn merge_base_remote(
    state: &AppState,
    project_id: &str,
    worktree_path: &str,
    base_branch: &str,
) -> Result<Option<String>, String> {
    let script = format!(
        "cd {wt} && git merge-base HEAD {origin} 2>/dev/null || git merge-base HEAD {local} 2>/dev/null",
        wt = shell_escape(worktree_path),
        origin = shell_escape(&format!("origin/{}", base_branch)),
        local = shell_escape(base_branch),
    );
    let (exit, out) = ssh_exec(state, project_id, &script).await?;
    if exit != Some(0) {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&out).trim().to_string();
    if text.is_empty() || text.contains(' ') {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

/// Blob content at `spec` ("HEAD:path" or "<mb-sha>:path") in a remote worktree.
async fn blob_at_remote(
    state: &AppState,
    project_id: &str,
    worktree_path: &str,
    spec: &str,
) -> (Option<String>, bool) {
    let script = format!(
        "cd {} && git show {}",
        shell_escape(worktree_path),
        shell_escape(spec)
    );
    let (exit, out) = match ssh_exec(state, project_id, &script).await {
        Ok(r) => r,
        Err(_) => return (None, false),
    };
    if exit != Some(0) {
        return (None, false);
    }
    if is_binary(&out) {
        return (None, true);
    }
    match String::from_utf8(out) {
        Ok(text) => (Some(text), false),
        Err(_) => (None, true),
    }
}

// ---------------------------------------------------------------------------
// Unified content access
// ---------------------------------------------------------------------------

async fn project_is_ssh(state: &AppState, project_id: &str) -> Result<bool, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;
    Ok(matches!(project.connection, Connection::Ssh { .. }))
}

fn file_content_local(
    worktree_path: &str,
    rel: &str,
    base_branch: Option<&str>,
) -> Result<DiffFileContent, String> {
    let (base, modified, base_bin, mod_bin) = match base_branch {
        Some(base) => {
            let mb = merge_base_local(worktree_path, base)?
                .ok_or_else(|| format!("No merge base with {}", base))?;
            let (base, base_bin) = match blob_at_local(worktree_path, &mb, rel)? {
                Some((data, bin)) => match String::from_utf8(data) {
                    Ok(text) => (Some(text), bin),
                    Err(_) => (None, true),
                },
                None => (None, false),
            };
            let (modified, mod_bin) = match blob_at_local(worktree_path, "HEAD", rel)? {
                Some((data, bin)) => match String::from_utf8(data) {
                    Ok(text) => (Some(text), bin),
                    Err(_) => (None, true),
                },
                None => (None, false),
            };
            (base, modified, base_bin, mod_bin)
        }
        None => {
            let (base, base_bin) = match blob_at_local(worktree_path, "HEAD", rel)? {
                Some((data, bin)) => match String::from_utf8(data) {
                    Ok(text) => (Some(text), bin),
                    Err(_) => (None, true),
                },
                None => (None, false),
            };
            let (modified, mod_bin) = modified_content_local(worktree_path, rel);
            (base, modified, base_bin, mod_bin)
        }
    };
    Ok(DiffFileContent {
        path: rel.to_string(),
        base,
        modified,
        binary: base_bin || mod_bin,
    })
}

async fn file_content(
    state: &AppState,
    project_id: &str,
    worktree_path: &str,
    rel: &str,
    base_branch: Option<&str>,
) -> Result<DiffFileContent, String> {
    if project_is_ssh(state, project_id).await? {
        let provider = crate::get_fs_provider(project_id, state).await?;
        let (base, modified, base_bin, mod_bin) = if let Some(base) = base_branch {
            let mb = merge_base_remote(state, project_id, worktree_path, base)
                .await?
                .ok_or_else(|| format!("No merge base with {}", base))?;
            let (base, base_bin) =
                blob_at_remote(state, project_id, worktree_path, &format!("{}:{}", mb, rel)).await;
            let (modified, mod_bin) =
                blob_at_remote(state, project_id, worktree_path, &format!("HEAD:{}", rel)).await;
            (base, modified, base_bin, mod_bin)
        } else {
            let (base, base_bin) =
                blob_at_remote(state, project_id, worktree_path, &format!("HEAD:{}", rel)).await;
            let abs = format!("{}/{}", worktree_path.trim_end_matches('/'), rel);
            let (modified, mod_bin) = match provider.read_file(&abs).await {
                Ok(text) => {
                    if text.contains('\0') {
                        (None, true)
                    } else {
                        (Some(text), false)
                    }
                }
                Err(_) => {
                    if provider.exists(&abs).await {
                        (None, true)
                    } else {
                        (None, false)
                    }
                }
            };
            (base, modified, base_bin, mod_bin)
        };
        return Ok(DiffFileContent {
            path: rel.to_string(),
            base,
            modified,
            binary: base_bin || mod_bin,
        });
    }

    file_content_local(worktree_path, rel, base_branch)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

pub async fn cmd_git_diff_summary(
    state: &AppState,
    project_id: String,
    worktree_path: String,
    base_branch: Option<String>,
) -> Result<Vec<DiffFileEntry>, String> {
    if project_is_ssh(state, &project_id).await? {
        remote_summary(state, &project_id, &worktree_path, base_branch.as_deref()).await
    } else {
        diff_summary_local(&worktree_path, base_branch.as_deref())
    }
}

pub async fn cmd_git_file_diff(
    state: &AppState,
    project_id: String,
    worktree_path: String,
    path: String,
    base_branch: Option<String>,
) -> Result<DiffFileContent, String> {
    file_content(state, &project_id, &worktree_path, &path, base_branch.as_deref()).await
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

static COMMENTS_LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();

fn comments_lock() -> &'static AsyncMutex<()> {
    COMMENTS_LOCK.get_or_init(|| AsyncMutex::new(()))
}

fn comments_file() -> Result<PathBuf, String> {
    Ok(crate::config::app_config_dir()?.join("diff_comments.json"))
}

fn load_comments_from(path: &Path) -> Vec<DiffComment> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_comments_to(path: &Path, comments: &[DiffComment]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let text =
        serde_json::to_string_pretty(comments).map_err(|e| format!("Failed to serialize: {}", e))?;
    std::fs::write(path, text).map_err(|e| format!("Failed to write comments: {}", e))
}

fn resolve_anchor(content: Option<&str>, line: u32, hash: &str) -> (Option<u32>, bool) {
    let Some(text) = content else {
        return (None, true);
    };
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return (None, true);
    }
    let hash_at = |l: u32| -> bool {
        lines
            .get(l as usize - 1)
            .map(|c| fnv1a64(c) == hash)
            .unwrap_or(false)
    };
    if hash_at(line) {
        return (Some(line), false);
    }
    // Prefer nearby lines, then any line with matching content.
    let start = line.saturating_sub(64).max(1);
    let end = (line + 64).min(lines.len() as u32);
    for l in start..=end {
        if l != line && hash_at(l) {
            return (Some(l), false);
        }
    }
    for l in 1..=lines.len() as u32 {
        if hash_at(l) {
            return (Some(l), false);
        }
    }
    (None, true)
}

async fn mutate_comments(
    f: impl FnOnce(&mut Vec<DiffComment>) -> Result<(), String>,
) -> Result<(), String> {
    let _guard = comments_lock().lock().await;
    let path = comments_file()?;
    let mut comments = load_comments_from(&path);
    let result = f(&mut comments)?;
    save_comments_to(&path, &comments)?;
    Ok(result)
}

async fn anchored_hash(
    state: &AppState,
    project_id: &str,
    worktree_path: &str,
    file: &str,
    side: &str,
    line: u32,
) -> Result<String, String> {
    let content = file_content(state, project_id, worktree_path, file, None).await?;
    let text = match side {
        "base" => content.base,
        _ => content.modified,
    };
    let text = text.ok_or("File content unavailable for anchoring")?;
    let line_text = text
        .lines()
        .nth(line as usize - 1)
        .ok_or("Line out of range")?;
    Ok(fnv1a64(line_text))
}

pub async fn cmd_diff_comments_list(
    state: &AppState,
    project_id: String,
    branch: String,
    worktree_path: String,
) -> Result<Vec<DiffComment>, String> {
    let _guard = comments_lock().lock().await;
    let all = load_comments_from(&comments_file()?);
    let mut resolved = Vec::new();
    let mut content_cache: HashMap<String, DiffFileContent> = HashMap::new();
    for c in all.iter().filter(|c| c.project_id == project_id && c.branch == branch) {
        let content = match content_cache.get(&c.file) {
            Some(cached) => cached.clone(),
            None => match file_content(state, &project_id, &worktree_path, &c.file, None).await {
                Ok(content) => {
                    content_cache.insert(c.file.clone(), content.clone());
                    content
                }
                Err(_) => DiffFileContent {
                    path: c.file.clone(),
                    base: None,
                    modified: None,
                    binary: false,
                },
            },
        };
        let text = match c.side.as_str() {
            "base" => content.base.clone(),
            _ => content.modified.clone(),
        };
        let (resolved_line, orphan) = resolve_anchor(text.as_deref(), c.line, &c.anchor_hash);
        let mut comment = c.clone();
        comment.resolved_line = resolved_line;
        comment.orphan = orphan;
        resolved.push(comment);
    }
    resolved.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    Ok(resolved)
}

pub async fn cmd_diff_comment_add(
    state: &AppState,
    project_id: String,
    branch: String,
    file: String,
    side: String,
    line: u32,
    body: String,
) -> Result<Vec<DiffComment>, String> {
    if side != "base" && side != "modified" {
        return Err("side must be \"base\" or \"modified\"".to_string());
    }
    if body.trim().is_empty() {
        return Err("Comment body is empty".to_string());
    }
    // Resolve worktree path for the branch so anchoring reads the right copy.
    let worktree_path = worktree_for_branch(state, &project_id, &branch).await?;
    let anchor_hash =
        anchored_hash(state, &project_id, &worktree_path, &file, &side, line).await?;
    let pid = project_id.clone();
    let br = branch.clone();
    mutate_comments(move |comments| {
        let now = now_millis();
        comments.push(DiffComment {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: pid,
            branch: br,
            file,
            side,
            line,
            anchor_hash,
            body: body.trim().to_string(),
            created_at: now,
            updated_at: now,
            resolved_line: Some(line),
            orphan: false,
        });
        Ok(())
    })
    .await?;
    cmd_diff_comments_list(state, project_id, branch, worktree_path).await
}

pub async fn cmd_diff_comment_update(
    state: &AppState,
    comment_id: String,
    body: String,
) -> Result<Vec<DiffComment>, String> {
    if body.trim().is_empty() {
        return Err("Comment body is empty".to_string());
    }
    let (project_id, branch) = {
        let _guard = comments_lock().lock().await;
        let all = load_comments_from(&comments_file()?);
        match all.iter().find(|c| c.id == comment_id) {
            Some(c) => (c.project_id.clone(), c.branch.clone()),
            None => return Err("Comment not found".to_string()),
        }
    };
    mutate_comments(|comments| {
        let comment = comments
            .iter_mut()
            .find(|c| c.id == comment_id)
            .ok_or("Comment not found")?;
        comment.body = body.trim().to_string();
        comment.updated_at = now_millis();
        Ok(())
    })
    .await?;
    let worktree_path = worktree_for_branch(state, &project_id, &branch).await?;
    cmd_diff_comments_list(state, project_id, branch, worktree_path).await
}

pub async fn cmd_diff_comment_delete(
    state: &AppState,
    comment_id: String,
) -> Result<Vec<DiffComment>, String> {
    let (project_id, branch) = {
        let _guard = comments_lock().lock().await;
        let all = load_comments_from(&comments_file()?);
        match all.iter().find(|c| c.id == comment_id) {
            Some(c) => (c.project_id.clone(), c.branch.clone()),
            None => return Err("Comment not found".to_string()),
        }
    };
    mutate_comments(|comments| {
        let len_before = comments.len();
        comments.retain(|c| c.id != comment_id);
        if comments.len() == len_before {
            return Err("Comment not found".to_string());
        }
        Ok(())
    })
    .await?;
    let worktree_path = worktree_for_branch(state, &project_id, &branch).await?;
    cmd_diff_comments_list(state, project_id, branch, worktree_path).await
}

async fn worktree_for_branch(
    state: &AppState,
    project_id: &str,
    branch: &str,
) -> Result<String, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;
    project
        .worktrees
        .iter()
        .find(|w| w.branch == branch)
        .map(|w| w.path.clone())
        .ok_or_else(|| format!("No worktree found for branch {}", branch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_known_vectors() {
        assert_eq!(fnv1a64(""), "cbf29ce484222325");
        assert_eq!(fnv1a64("a"), "af63dc4c8601ec8c");
        assert_eq!(fnv1a64("foobar"), "85944171f73967e8");
    }

    #[test]
    fn resolve_anchor_hits_direct_line() {
        let content = "one\ntwo\nthree\n";
        let hash = fnv1a64("two");
        assert_eq!(resolve_anchor(Some(content), 2, &hash), (Some(2), false));
    }

    #[test]
    fn resolve_anchor_follows_shifted_line() {
        let before = "a\nb\nc\nd\n";
        let after = "a\nx\ny\nb\nc\nd\n";
        let hash = fnv1a64("b");
        let (resolved, orphan) = resolve_anchor(Some(after), 2, &hash);
        assert_eq!((resolved, orphan), (Some(4), false));
    }

    #[test]
    fn resolve_anchor_orphans_on_missing_content() {
        let hash = fnv1a64("gone");
        assert_eq!(resolve_anchor(Some("a\nb\n"), 1, &hash), (None, true));
        assert_eq!(resolve_anchor(None, 1, &hash), (None, true));
        assert_eq!(resolve_anchor(Some(""), 1, &hash), (None, true));
    }

    #[test]
    fn parse_porcelain_v1_z_statuses() {
        let data = b" M b.txt\0R  c.txt\0a.txt\0?? new.txt\0A  staged.txt\0DU merged.txt\0";
        let entries = parse_porcelain_v1_z(data);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].path, "b.txt");
        assert_eq!(entries[0].status, "modified");
        assert_eq!(entries[1].path, "c.txt");
        assert_eq!(entries[1].status, "renamed");
        assert_eq!(entries[1].old_path.as_deref(), Some("a.txt"));
        assert_eq!(entries[2].path, "new.txt");
        assert_eq!(entries[2].status, "added");
        assert_eq!(entries[3].path, "staged.txt");
        assert_eq!(entries[3].status, "added");
        assert_eq!(entries[4].path, "merged.txt");
        assert_eq!(entries[4].status, "conflicted");
    }

    #[test]
    fn parse_numstat_plain_and_rename() {
        let data = "1\t0\tb.txt\n0\t0\ta.txt => c.txt\n3\t0\told name.txt\n";
        let map = parse_numstat(data);
        assert_eq!(map.get("b.txt"), Some(&(1, 0)));
        assert_eq!(map.get("c.txt"), Some(&(0, 0)));
        assert_eq!(map.get("old name.txt"), Some(&(3, 0)));
        assert!(!map.contains_key("a.txt"));
    }

    #[test]
    fn parse_numstat_braced_rename() {
        let data = "2\t1\tsrc/{old => new}/mod.rs\n-\t-\tbin.img\n";
        let map = parse_numstat(data);
        assert_eq!(map.get("src/new/mod.rs"), Some(&(2, 1)));
        assert_eq!(map.get("bin.img"), Some(&(0, 0)));
    }

    #[test]
    fn local_summary_and_content_e2e() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {:?} failed: {:?}", args, out.stderr);
        };
        run(&["init", "-q", "-b", "master", "."]);
        std::fs::write(root.join("keep.txt"), "a\nb\nc\n").unwrap();
        std::fs::write(root.join("mod.txt"), "old\n").unwrap();
        std::fs::write(root.join("del.txt"), "bye\n").unwrap();
        run(&["add", "."]);
        run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init"]);
        std::fs::write(root.join("mod.txt"), "a2\nb\nc\n").unwrap();
        std::fs::remove_file(root.join("del.txt")).unwrap();
        std::fs::write(root.join("new.txt"), "brand\n").unwrap();

        let summary = diff_summary_local(root.to_str().unwrap(), None).unwrap();
        let by_path: HashMap<&str, &DiffFileEntry> =
            summary.iter().map(|e| (e.path.as_str(), e)).collect();
        assert_eq!(by_path["mod.txt"].status, "modified");
        assert_eq!(by_path["mod.txt"].insertions, 3);
        assert_eq!(by_path["mod.txt"].deletions, 1);
        assert_eq!(by_path["del.txt"].status, "deleted");
        assert_eq!(by_path["new.txt"].status, "added");
        assert!(by_path.get("keep.txt").is_none());

        let content = file_content_local(root.to_str().unwrap(), "mod.txt", None).unwrap();
        assert_eq!(content.base.as_deref(), Some("old\n"));
        assert_eq!(content.modified.as_deref(), Some("a2\nb\nc\n"));
        assert!(!content.binary);

        let deleted = file_content_local(root.to_str().unwrap(), "del.txt", None).unwrap();
        assert_eq!(deleted.base.as_deref(), Some("bye\n"));
        assert_eq!(deleted.modified, None);

        let added = file_content_local(root.to_str().unwrap(), "new.txt", None).unwrap();
        assert_eq!(added.base, None);
        assert_eq!(added.modified.as_deref(), Some("brand\n"));

        // Branch mode: commit the changes, then diff the branch against master.
        run(&["checkout", "-qb", "feat"]);
        run(&["add", "-A"]);
        run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "c1"]);
        let branch_summary = diff_summary_local(root.to_str().unwrap(), Some("master")).unwrap();
        let by_status: HashMap<&str, &DiffFileEntry> =
            branch_summary.iter().map(|e| (e.path.as_str(), e)).collect();
        assert_eq!(by_status["mod.txt"].status, "modified");
        assert_eq!(by_status["del.txt"].status, "deleted");
        assert_eq!(by_status["new.txt"].status, "added");
        assert!(by_status.get("keep.txt").is_none());

        let branch_content =
            file_content_local(root.to_str().unwrap(), "mod.txt", Some("master")).unwrap();
        assert_eq!(branch_content.base.as_deref(), Some("old\n"));
        assert_eq!(branch_content.modified.as_deref(), Some("a2\nb\nc\n"));
    }

    #[test]
    fn comment_crud_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("diff_comments.json");
        assert!(load_comments_from(&path).is_empty());

        let now = now_millis();
        save_comments_to(
            &path,
            &[DiffComment {
                id: "c1".to_string(),
                project_id: "p1".to_string(),
                branch: "feature".to_string(),
                file: "src/main.rs".to_string(),
                side: "modified".to_string(),
                line: 3,
                anchor_hash: "abc".to_string(),
                body: "hello".to_string(),
                created_at: now,
                updated_at: now,
                resolved_line: Some(3),
                orphan: false,
            }],
        )
        .unwrap();

        let loaded = load_comments_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].body, "hello");

        // Unknown project/branch filter yields nothing.
        let mut all = load_comments_from(&path);
        all.retain(|c| c.project_id == "p1" && c.branch == "other");
        assert!(all.is_empty());
    }
}
