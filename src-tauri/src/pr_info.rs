use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use crate::{load_projects, get_repo_path, AppState, Connection};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrState {
    Open,
    Merged,
    Closed,
    Draft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrProvider {
    Github,
    Bitbucket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrInfo {
    pub number: String,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub author: String,
    pub source_branch: String,
    pub target_branch: String,
    pub created_at: String,
    pub updated_at: String,
    pub provider: PrProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrInfoResult {
    pub pr: Option<PrInfo>,
    pub provider: Option<String>,
    pub error: Option<String>,
}

// ── Provider detection (local) ──────────────────────────────────────────────

fn detect_remote_host_local(repo_path: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args([
            "-C",
            &repo_path.to_string_lossy(),
            "remote",
            "get-url",
            "origin",
        ])
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        return Err("Not a git repository or no origin remote configured".to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_lowercase())
}

fn provider_from_url(remote_url: &str) -> Result<&'static str, String> {
    if remote_url.contains("github.com") {
        Ok("github")
    } else if remote_url.contains("bitbucket.org") {
        Ok("bitbucket")
    } else {
        Err(format!(
            "Unsupported git host. Remote URL: {}. Supported hosts: github.com, bitbucket.org",
            remote_url
        ))
    }
}

// ── Shell escape ──────────────────────────────────────────────────────────

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

// ── Remote command execution via SSH ────────────────────────────────────────

async fn exec_remote(
    state: &AppState,
    project_id: &str,
    cmd: &str,
) -> Result<String, String> {
    let connections = state.ssh_connections.lock().await;
    let conn = connections
        .get(project_id)
        .ok_or("No SSH connection found for this project")?;

    let mut channel = conn
        .session
        .channel_open_session()
        .await
        .map_err(|e| format!("Failed to open SSH channel: {}", e))?;

    channel
        .exec(false, cmd)
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
            }
            russh::ChannelMsg::Close => break,
            _ => {}
        }
        if exit_status.is_some() {
            break;
        }
    }

    match exit_status {
        Some(0) => Ok(stdout),
        Some(code) => Err(format!(
            "Remote command failed (exit {}): {}",
            code,
            stderr.trim()
        )),
        None => Err("Remote command: no exit status received".to_string()),
    }
}

async fn detect_remote_host_ssh(
    state: &AppState,
    project_id: &str,
    repo_path: &str,
) -> Result<String, String> {
    let repo_quoted = shell_escape(repo_path);
    let cmd = format!("cd {} && git remote get-url origin", repo_quoted);
    let output = exec_remote(state, project_id, &cmd).await?;
    Ok(output.trim().to_lowercase())
}

// ── Local CLI runner ──────────────────────────────────────────────────────────

fn run_cli_local(repo_path: &str, bin: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(repo_path);
    let output = cmd
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("{} CLI not found. Is it installed and on your PATH?", bin)
            } else {
                format!("Failed to run {}: {}", bin, e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("{} exited with error: {}", bin, stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn run_cli_remote(
    state: &AppState,
    project_id: &str,
    repo_path: &str,
    bin: &str,
    args: &[&str],
) -> Result<String, String> {
    let args_quoted: Vec<String> = args.iter().map(|a| shell_escape(a)).collect();
    let repo_quoted = shell_escape(repo_path);
    let cmd = format!("cd {} && {} {}", repo_quoted, bin, args_quoted.join(" "));
    exec_remote(state, project_id, &cmd).await
}

// ── GitHub JSON parsing ──────────────────────────────────────────────────────

fn parse_gh_pr(json: &str) -> Result<PrInfo, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GhPr {
        number: serde_json::Value,
        title: String,
        url: String,
        state: String,
        author: Option<GhAuthor>,
        #[serde(rename = "headRefName")]
        head_ref_name: String,
        #[serde(rename = "baseRefName")]
        base_ref_name: String,
        #[serde(rename = "createdAt")]
        created_at: String,
        #[serde(rename = "updatedAt")]
        updated_at: String,
    }

    #[derive(Deserialize)]
    struct GhAuthor {
        login: String,
    }

    let pr: GhPr =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse gh output: {}", e))?;

    Ok(PrInfo {
        number: normalize_json_value(&pr.number),
        title: pr.title,
        url: pr.url,
        state: parse_gh_state(&pr.state),
        author: pr
            .author
            .map(|a| a.login)
            .unwrap_or_else(|| "unknown".to_string()),
        source_branch: pr.head_ref_name,
        target_branch: pr.base_ref_name,
        created_at: pr.created_at,
        updated_at: pr.updated_at,
        provider: PrProvider::Github,
    })
}

fn parse_gh_pr_list(json: &str) -> Result<Vec<PrInfo>, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GhPr {
        number: serde_json::Value,
        title: String,
        url: String,
        state: String,
        author: Option<GhAuthor>,
        #[serde(rename = "headRefName")]
        head_ref_name: String,
        #[serde(rename = "baseRefName")]
        base_ref_name: String,
        #[serde(rename = "createdAt")]
        created_at: String,
        #[serde(rename = "updatedAt")]
        updated_at: String,
    }

    #[derive(Deserialize)]
    struct GhAuthor {
        login: String,
    }

    let prs: Vec<GhPr> =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse gh output: {}", e))?;

    prs.into_iter()
        .map(|pr| {
            Ok(PrInfo {
                number: normalize_json_value(&pr.number),
                title: pr.title,
                url: pr.url,
                state: parse_gh_state(&pr.state),
                author: pr
                    .author
                    .map(|a| a.login)
                    .unwrap_or_else(|| "unknown".to_string()),
                source_branch: pr.head_ref_name,
                target_branch: pr.base_ref_name,
                created_at: pr.created_at,
                updated_at: pr.updated_at,
                provider: PrProvider::Github,
            })
        })
        .collect()
}

// ── Bitbucket JSON parsing ────────────────────────────────────────────────

fn parse_bkt_pr_single(json: &str, target_branch: &str) -> Result<Option<PrInfo>, String> {
    let prs = parse_bkt_pr_list(json)?;
    Ok(prs.into_iter().find(|p| p.source_branch == target_branch))
}

fn parse_bkt_pr_list(json: &str) -> Result<Vec<PrInfo>, String> {
    let json = json.trim();

    // bkt may wrap the array in an object with a "values" or "pullrequests" key.
    // Try to extract the array from a wrapper object first.
    let array_json = if json.starts_with('{') {
        if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(json) {
            let arr = wrapper
                .get("values")
                .or_else(|| wrapper.get("pullrequests"))
                .or_else(|| wrapper.get("pull_requests"))
                .or_else(|| wrapper.get("items"));
            match arr {
                Some(serde_json::Value::Array(_)) => arr.cloned(),
                _ => {
                    let keys: Vec<&str> = wrapper.as_object().map(|o| o.keys().map(|k| k.as_str()).collect()).unwrap_or_default();
                    return Err(format!("bkt output is an object but no array found. Available keys: {:?}. First 500 chars: {}", keys, &json[..json.len().min(500)]));
                }
            }
        } else {
            return Err("Failed to parse bkt output as JSON".to_string());
        }
    } else {
        None
    };

    let json_str: &str;
    let json_owned: String;
    if let Some(ref arr) = array_json {
        json_owned = arr.to_string();
        json_str = &json_owned;
    } else {
        json_str = json;
    };
    #[derive(Deserialize)]
    struct BktPr {
        id: serde_json::Value,
        title: String,
        state: Option<String>,
        draft: Option<bool>,
        author: Option<BktAuthor>,
        source: Option<BktBranch>,
        destination: Option<BktBranch>,
        created_on: Option<String>,
        updated_on: Option<String>,
        links: Option<BktLinks>,
    }

    #[derive(Deserialize)]
    struct BktLinks {
        html: Option<BktHref>,
    }

    #[derive(Deserialize)]
    struct BktHref {
        href: String,
    }

    #[derive(Deserialize)]
    struct BktAuthor {
        display_name: Option<String>,
        username: Option<String>,
    }

    #[derive(Deserialize)]
    struct BktBranch {
        branch: Option<BktBranchName>,
    }

    #[derive(Deserialize)]
    struct BktBranchName {
        name: String,
    }

    let prs: Vec<BktPr> = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse bkt output: {}", e))?;

    prs.into_iter()
        .map(|pr| {
            let url = pr
                .links
                .and_then(|l| l.html)
                .map(|h| h.href)
                .unwrap_or_else(|| format!("#{}", normalize_json_value(&pr.id)));

            let state = if pr.draft == Some(true) {
                PrState::Draft
            } else {
                parse_bkt_state(pr.state.as_deref())
            };

            Ok(PrInfo {
                number: normalize_json_value(&pr.id),
                title: pr.title,
                url,
                state,
                author: pr
                    .author
                    .and_then(|a| a.display_name.or(a.username))
                    .unwrap_or_else(|| "unknown".to_string()),
                source_branch: pr
                    .source
                    .and_then(|b| b.branch)
                    .map(|b| b.name)
                    .unwrap_or_else(|| "unknown".to_string()),
                target_branch: pr
                    .destination
                    .and_then(|b| b.branch)
                    .map(|b| b.name)
                    .unwrap_or_else(|| "unknown".to_string()),
                created_at: pr.created_on.unwrap_or_default(),
                updated_at: pr.updated_on.unwrap_or_default(),
                provider: PrProvider::Bitbucket,
            })
        })
        .collect()
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn normalize_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}

fn parse_gh_state(state: &str) -> PrState {
    match state.to_lowercase().as_str() {
        "open" => PrState::Open,
        "merged" => PrState::Merged,
        "closed" => PrState::Closed,
        _ => PrState::Open,
    }
}

fn parse_bkt_state(state: Option<&str>) -> PrState {
    match state.unwrap_or("open").to_lowercase().as_str() {
        "open" => PrState::Open,
        "merged" => PrState::Merged,
        "declined" | "closed" => PrState::Closed,
        _ => PrState::Open,
    }
}

// ── Tauri commands ────────────────────────────────────────────────

#[tauri::command]
pub async fn pr_for_branch(
    project_id: String,
    branch: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<PrInfoResult, String> {
    let app_handle = state.app_handle.lock().unwrap().clone().ok_or("App handle not available")?;
    let projects = load_projects(app_handle)?;
    let project = projects.iter().find(|p| p.id == project_id).ok_or("Project not found")?;

    let repo_path = get_repo_path(project);

    match &project.connection {
        Connection::Local { .. } => {
            let remote_url = detect_remote_host_local(Path::new(&repo_path))?;
            let provider = provider_from_url(&remote_url)?;

            match provider {
                "github" => {
                    let output = match run_cli_local(
                        &repo_path,
                        "gh",
                        &[
                            "pr",
                            "view",
                            &branch,
                            "--json",
                            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt",
                        ],
                    ) {
                        Ok(out) => out,
                        Err(e) => {
                            let lower = e.to_lowercase();
                            if lower.contains("no pull request") || lower.contains("no pr") {
                                return Ok(PrInfoResult {
                                    pr: None,
                                    provider: Some("github".to_string()),
                                    error: Some(format!("No PR found for branch '{}'", branch)),
                                });
                            }
                            return Err(e);
                        }
                    };
                    if output.trim().is_empty() {
                        return Ok(PrInfoResult {
                            pr: None,
                            provider: Some("github".to_string()),
                            error: Some(format!("No PR found for branch '{}'", branch)),
                        });
                    }
                    match parse_gh_pr(&output) {
                        Ok(pr) => Ok(PrInfoResult {
                            pr: Some(pr),
                            provider: Some("github".to_string()),
                            error: None,
                        }),
                        Err(e) => Ok(PrInfoResult {
                            pr: None,
                            provider: Some("github".to_string()),
                            error: Some(e),
                        }),
                    }
                }
                "bitbucket" => {
                    let output = run_cli_local(&repo_path, "bkt", &["pr", "list", "--json"])?;
                    if output.trim().is_empty() {
                        return Ok(PrInfoResult {
                            pr: None,
                            provider: Some("bitbucket".to_string()),
                            error: Some(format!("No PR found for branch '{}'", branch)),
                        });
                    }
                    match parse_bkt_pr_single(&output, &branch) {
                        Ok(Some(pr)) => Ok(PrInfoResult {
                            pr: Some(pr),
                            provider: Some("bitbucket".to_string()),
                            error: None,
                        }),
                        Ok(None) => Ok(PrInfoResult {
                            pr: None,
                            provider: Some("bitbucket".to_string()),
                            error: Some(format!("No PR found for branch '{}'", branch)),
                        }),
                        Err(e) => Ok(PrInfoResult {
                            pr: None,
                            provider: Some("bitbucket".to_string()),
                            error: Some(e),
                        }),
                    }
                }
                _ => unreachable!(),
            }
        }
        Connection::Ssh { .. } => {
            let remote_url = detect_remote_host_ssh(&state, &project_id, &repo_path).await?;
            let provider = provider_from_url(&remote_url)?;

            match provider {
                "github" => {
                    let output = match run_cli_remote(
                        &state,
                        &project_id,
                        &repo_path,
                        "gh",
                        &[
                            "pr",
                            "view",
                            &branch,
                            "--json",
                            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt",
                        ],
                    )
                    .await {
                        Ok(out) => out,
                        Err(e) => {
                            let lower = e.to_lowercase();
                            if lower.contains("no pull request") || lower.contains("no pr") {
                                return Ok(PrInfoResult {
                                    pr: None,
                                    provider: Some("github".to_string()),
                                    error: Some(format!("No PR found for branch '{}'", branch)),
                                });
                            }
                            return Err(e);
                        }
                    };
                    if output.trim().is_empty() {
                        return Ok(PrInfoResult {
                            pr: None,
                            provider: Some("github".to_string()),
                            error: Some(format!("No PR found for branch '{}'", branch)),
                        });
                    }
                    match parse_gh_pr(&output) {
                        Ok(pr) => Ok(PrInfoResult {
                            pr: Some(pr),
                            provider: Some("github".to_string()),
                            error: None,
                        }),
                        Err(e) => Ok(PrInfoResult {
                            pr: None,
                            provider: Some("github".to_string()),
                            error: Some(e),
                        }),
                    }
                }
                "bitbucket" => {
                    let output = run_cli_remote(
                        &state,
                        &project_id,
                        &repo_path,
                        "bkt",
                        &["pr", "list", "--json"],
                    )
                    .await?;
                    if output.trim().is_empty() {
                        return Ok(PrInfoResult {
                            pr: None,
                            provider: Some("bitbucket".to_string()),
                            error: Some(format!("No PR found for branch '{}'", branch)),
                        });
                    }
                    match parse_bkt_pr_single(&output, &branch) {
                        Ok(Some(pr)) => Ok(PrInfoResult {
                            pr: Some(pr),
                            provider: Some("bitbucket".to_string()),
                            error: None,
                        }),
                        Ok(None) => Ok(PrInfoResult {
                            pr: None,
                            provider: Some("bitbucket".to_string()),
                            error: Some(format!("No PR found for branch '{}'", branch)),
                        }),
                        Err(e) => Ok(PrInfoResult {
                            pr: None,
                            provider: Some("bitbucket".to_string()),
                            error: Some(e),
                        }),
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}

#[tauri::command]
pub async fn pr_list_for_repo(
    project_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<PrInfo>, String> {
    let app_handle = state.app_handle.lock().unwrap().clone().ok_or("App handle not available")?;
    let projects = load_projects(app_handle)?;
    let project = projects.iter().find(|p| p.id == project_id).ok_or("Project not found")?;

    let repo_path = get_repo_path(project);

    match &project.connection {
        Connection::Local { .. } => {
            let remote_url = detect_remote_host_local(Path::new(&repo_path))?;
            let provider = provider_from_url(&remote_url)?;

            match provider {
                "github" => {
                    let output = run_cli_local(
                        &repo_path,
                        "gh",
                        &[
                            "pr",
                            "list",
                            "--json",
                            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt",
                        ],
                    )?;
                    if output.trim().is_empty() {
                        return Ok(vec![]);
                    }
                    parse_gh_pr_list(&output)
                }
                "bitbucket" => {
                    let output = run_cli_local(&repo_path, "bkt", &["pr", "list", "--json"])?;
                    if output.trim().is_empty() {
                        return Ok(vec![]);
                    }
                    parse_bkt_pr_list(&output)
                }
                _ => unreachable!(),
            }
        }
        Connection::Ssh { .. } => {
            let remote_url = detect_remote_host_ssh(&state, &project_id, &repo_path).await?;
            let provider = provider_from_url(&remote_url)?;

            match provider {
                "github" => {
                    let output = run_cli_remote(
                        &state,
                        &project_id,
                        &repo_path,
                        "gh",
                        &[
                            "pr",
                            "list",
                            "--json",
                            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt",
                        ],
                    )
                    .await?;
                    if output.trim().is_empty() {
                        return Ok(vec![]);
                    }
                    parse_gh_pr_list(&output)
                }
                "bitbucket" => {
                    let output = run_cli_remote(
                        &state,
                        &project_id,
                        &repo_path,
                        "bkt",
                        &["pr", "list", "--json"],
                    )
                    .await?;
                    if output.trim().is_empty() {
                        return Ok(vec![]);
                    }
                    parse_bkt_pr_list(&output)
                }
                _ => unreachable!(),
            }
        }
    }
}