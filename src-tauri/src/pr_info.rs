use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use crate::{get_repo_path, AppState, Connection};

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
pub enum CheckStatus {
    Success,
    Pending,
    Failure,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrReview {
    pub author: String,
    /// approved | changes_requested | commented | dismissed | pending
    pub state: String,
    pub submitted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrComment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub outdated: bool,
    pub resolved: bool,
    pub file_path: Option<String>,
    /// The signed-in viewer has commented on the thread this comment belongs to.
    pub viewer_replied: bool,
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
    pub check_status: CheckStatus,
    #[serde(default)]
    pub reviews: Vec<PrReview>,
    /// Reviewers requested but who have not reviewed yet (missing approvals).
    #[serde(default)]
    pub review_requests: Vec<String>,
    #[serde(default)]
    pub comments: Vec<PrComment>,
    #[serde(default)]
    pub viewer_login: Option<String>,
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

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_lowercase())
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

async fn exec_remote(state: &AppState, project_id: &str, cmd: &str) -> Result<String, String> {
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
            russh::ChannelMsg::ExitStatus {
                exit_status: status,
            } => {
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
    let output = cmd.output().map_err(|e| {
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

fn aggregate_gh_check_rollup(rollup: &[serde_json::Value]) -> CheckStatus {
    if rollup.is_empty() {
        return CheckStatus::Unknown;
    }
    let mut any_pending = false;
    for item in rollup {
        let typename = item
            .get("__typename")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if typename == "CheckRun" {
            let status = item
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_uppercase();
            let conclusion = item
                .get("conclusion")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_uppercase();
            if status != "COMPLETED" {
                any_pending = true;
            } else if matches!(
                conclusion.as_str(),
                "FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE"
            ) {
                return CheckStatus::Failure;
            } else if conclusion.is_empty() {
                any_pending = true;
            }
        } else {
            let state = item
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_uppercase();
            match state.as_str() {
                "FAILURE" | "ERROR" => return CheckStatus::Failure,
                "PENDING" | "EXPECTED" => any_pending = true,
                _ => {}
            }
        }
    }
    if any_pending {
        CheckStatus::Pending
    } else {
        CheckStatus::Success
    }
}

fn parse_gh_pr(json: &str) -> Result<PrInfo, String> {
    #[derive(Deserialize)]
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
        #[serde(rename = "statusCheckRollup")]
        status_check_rollup: Option<Vec<serde_json::Value>>,
        #[serde(rename = "latestReviews", default)]
        latest_reviews: Option<Vec<GhReview>>,
        #[serde(default)]
        reviews: Option<Vec<GhReview>>,
        #[serde(rename = "reviewRequests", default)]
        review_requests: Option<Vec<serde_json::Value>>,
        #[serde(default)]
        comments: Option<Vec<GhComment>>,
    }

    #[derive(Deserialize)]
    struct GhAuthor {
        login: String,
    }

    #[derive(Deserialize)]
    struct GhReview {
        author: Option<GhAuthor>,
        state: String,
        #[serde(rename = "submittedAt")]
        submitted_at: Option<String>,
    }

    #[derive(Deserialize)]
    struct GhComment {
        author: Option<GhAuthor>,
        body: String,
        #[serde(rename = "createdAt")]
        created_at: Option<String>,
    }

    fn gh_login(v: &serde_json::Value) -> Option<String> {
        v.get("login")
            .or_else(|| v.get("slug"))
            .and_then(|l| l.as_str())
            .map(|s| s.to_string())
    }

    fn aggregate_gh_reviews(
        latest: Option<Vec<GhReview>>,
        fallback: Option<Vec<GhReview>>,
    ) -> Vec<PrReview> {
        let source = match (latest, fallback) {
            (Some(v), _) if !v.is_empty() => v,
            (Some(_), Some(v)) => v,
            (None, Some(v)) => v,
            _ => vec![],
        };
        source
            .into_iter()
            .map(|r| PrReview {
                author: r
                    .author
                    .map(|a| a.login)
                    .unwrap_or_else(|| "unknown".to_string()),
                state: match r.state.to_uppercase().as_str() {
                    "APPROVED" => "approved",
                    "CHANGES_REQUESTED" => "changes_requested",
                    "DISMISSED" => "dismissed",
                    "PENDING" => "pending",
                    _ => "commented",
                }
                .to_string(),
                submitted_at: r.submitted_at.unwrap_or_default(),
            })
            .collect()
    }

    fn gh_review_requests(v: Option<Vec<serde_json::Value>>) -> Vec<String> {
        v.unwrap_or_default()
            .iter()
            .filter_map(gh_login)
            .collect()
    }

    fn gh_issue_comments(v: Option<Vec<GhComment>>) -> Vec<PrComment> {
        v.unwrap_or_default()
            .into_iter()
            .map(|c| PrComment {
                id: String::new(),
                author: c
                    .author
                    .map(|a| a.login)
                    .unwrap_or_else(|| "unknown".to_string()),
                body: c.body,
                created_at: c.created_at.unwrap_or_default(),
                outdated: false,
                resolved: false,
                file_path: None,
                viewer_replied: false,
            })
            .collect()
    }

    let pr: GhPr =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse gh output: {}", e))?;

    let check_status = pr
        .status_check_rollup
        .as_deref()
        .map(aggregate_gh_check_rollup)
        .unwrap_or(CheckStatus::Unknown);

    let mut comments = gh_issue_comments(pr.comments);
    comments.sort_by(|a, b| b.created_at.cmp(&a.created_at));

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
        check_status,
        reviews: aggregate_gh_reviews(pr.latest_reviews, pr.reviews),
        review_requests: gh_review_requests(pr.review_requests),
        comments,
        viewer_login: None,
    })
}

fn parse_gh_pr_list(json: &str) -> Result<Vec<PrInfo>, String> {
    let prs: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse gh output: {}", e))?;

    prs.iter()
        .map(|pr| parse_gh_pr(&pr.to_string()))
        .collect()
}

// ── GitHub review threads (GraphQL) ─────────────────────────────────────────

const GH_THREADS_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){viewer{login}repository(owner:$owner,name:$repo){pullRequest(number:$number){reviewThreads(first:100){nodes{isOutdated isResolved comments(first:10){nodes{databaseId author{login} body createdAt path}}}}}}}";

fn parse_owner_repo(remote_url: &str) -> Option<(String, String)> {
    let url = remote_url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let rest = url
        .split_once("github.com:")
        .map(|(_, r)| r)
        .or_else(|| url.split_once("github.com/").map(|(_, r)| r))?;
    let mut parts = rest.splitn(2, '/');
    let owner = parts.next()?.trim().to_string();
    let repo = parts.next()?.trim().to_string();
    if owner.is_empty() || repo.is_empty() {
        None
    } else {
        Some((owner, repo))
    }
}

/// Returns (viewer login, review-thread comments). Best-effort: errors yield empty data.
fn parse_gh_threads(json: &str) -> (Option<String>, Vec<PrComment>) {
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return (None, vec![]),
    };
    let viewer = v
        .pointer("/data/viewer/login")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());

    let mut comments = vec![];
    if let Some(threads) = v
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(|n| n.as_array())
    {
        for thread in threads {
            let outdated = thread
                .get("isOutdated")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            let resolved = thread
                .get("isResolved")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            let nodes = thread
                .pointer("/comments/nodes")
                .and_then(|n| n.as_array())
                .cloned()
                .unwrap_or_default();
            let viewer_replied = viewer.as_deref().is_some_and(|viewer| {
                nodes.iter().any(|c| {
                    c.pointer("/author/login").and_then(|x| x.as_str()) == Some(viewer)
                })
            });
            for c in nodes {
                comments.push(PrComment {
                    id: c
                        .get("databaseId")
                        .map(normalize_json_value)
                        .unwrap_or_default(),
                    author: c
                        .pointer("/author/login")
                        .and_then(|x| x.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    body: c
                        .get("body")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    created_at: c
                        .get("createdAt")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    outdated,
                    resolved,
                    file_path: c
                        .get("path")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string()),
                    viewer_replied,
                });
            }
        }
    }
    comments.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    (viewer, comments)
}

fn gh_threads_args(owner: &str, repo: &str, number: &str) -> Vec<String> {
    vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={}", GH_THREADS_QUERY),
        "-f".to_string(),
        format!("owner={}", owner),
        "-f".to_string(),
        format!("repo={}", repo),
        "-F".to_string(),
        format!("number={}", number),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrThreadsResult {
    pub comments: Vec<PrComment>,
    pub viewer_login: Option<String>,
    pub error: Option<String>,
}

/// Fetches GitHub review threads on demand (PR panel only) to conserve API quota.
/// Returns a soft error instead of failing so the panel can degrade to base PR data.
pub async fn cmd_pr_threads_for_branch(
    state: &AppState,
    project_id: String,
    pr_number: String,
) -> Result<PrThreadsResult, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    let repo_path = get_repo_path(project);

    let (remote_url, is_local) = match &project.connection {
        Connection::Local { .. } => (detect_remote_host_local(Path::new(&repo_path))?, true),
        Connection::Ssh { .. } => (
            detect_remote_host_ssh(state, &project_id, &repo_path).await?,
            false,
        ),
    };

    let provider = match provider_from_url(&remote_url) {
        Ok(p) => p,
        Err(_) => {
            return Ok(PrThreadsResult {
                comments: vec![],
                viewer_login: None,
                error: Some(format!("Unsupported git host. Remote URL: {}", remote_url)),
            })
        }
    };
    if provider != "github" {
        return Ok(PrThreadsResult {
            comments: vec![],
            viewer_login: None,
            error: Some("Review threads are only supported for GitHub PRs".to_string()),
        });
    }
    let Some((owner, repo)) = parse_owner_repo(&remote_url) else {
        return Ok(PrThreadsResult {
            comments: vec![],
            viewer_login: None,
            error: Some("Could not determine GitHub owner/repo from the origin remote".to_string()),
        });
    };

    let args = gh_threads_args(&owner, &repo, &pr_number);
    let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let output = if is_local {
        run_cli_local(&repo_path, "gh", &args)
    } else {
        run_cli_remote(state, &project_id, &repo_path, "gh", &args).await
    };

    match output {
        Ok(out) => {
            let (viewer, comments) = parse_gh_threads(&out);
            Ok(PrThreadsResult {
                comments,
                viewer_login: viewer,
                error: None,
            })
        }
        Err(e) => Ok(PrThreadsResult {
            comments: vec![],
            viewer_login: None,
            error: Some(e),
        }),
    }
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
                    let keys: Vec<&str> = wrapper
                        .as_object()
                        .map(|o| o.keys().map(|k| k.as_str()).collect())
                        .unwrap_or_default();
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
        #[serde(default)]
        participants: Option<Vec<BktParticipant>>,
    }

    #[derive(Deserialize)]
    struct BktParticipant {
        #[serde(default)]
        approved: Option<bool>,
        #[serde(default)]
        role: Option<String>,
        user: Option<BktAuthor>,
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

    let prs: Vec<BktPr> =
        serde_json::from_str(json_str).map_err(|e| format!("Failed to parse bkt output: {}", e))?;

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
                check_status: CheckStatus::Unknown,
                reviews: pr
                    .participants
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|p| {
                        let name = p
                            .user
                            .and_then(|u| u.display_name.or(u.username))
                            .unwrap_or_else(|| "unknown".to_string());
                        if p.approved == Some(true) {
                            Some(PrReview {
                                author: name,
                                state: "approved".to_string(),
                                submitted_at: String::new(),
                            })
                        } else if p.role.as_deref() == Some("REVIEWER") {
                            Some(PrReview {
                                author: name,
                                state: "pending".to_string(),
                                submitted_at: String::new(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect(),
                review_requests: vec![],
                comments: vec![],
                viewer_login: None,
            })
        })
        .collect()
}

fn aggregate_bkt_checks(json: &str) -> CheckStatus {
    let v: serde_json::Value = match serde_json::from_str(json.trim()) {
        Ok(v) => v,
        Err(_) => return CheckStatus::Unknown,
    };

    if let Some(summary) = v.get("summary") {
        let count = |keys: &[&str]| -> u64 {
            keys.iter()
                .filter_map(|k| summary.get(k).and_then(|x| x.as_u64()))
                .sum()
        };
        let failed = count(&["failed", "failure", "error", "stopped"]);
        let in_progress = count(&["in_progress", "inProgress", "pending", "running", "paused"]);
        let succeeded = count(&["successful", "success", "passed"]);
        let total = count(&["total", "count"]);
        if failed > 0 {
            return CheckStatus::Failure;
        }
        if in_progress > 0 {
            return CheckStatus::Pending;
        }
        if succeeded > 0 || total > 0 {
            return CheckStatus::Success;
        }
        return CheckStatus::Unknown;
    }

    let statuses = v
        .get("statuses")
        .and_then(|s| s.as_array())
        .or_else(|| v.as_array());
    let Some(statuses) = statuses else {
        return CheckStatus::Unknown;
    };
    if statuses.is_empty() {
        return CheckStatus::Unknown;
    }
    let mut any_pending = false;
    for s in statuses {
        let state = s
            .get("state")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_uppercase();
        match state.as_str() {
            "FAILED" | "FAILURE" | "ERROR" | "STOPPED" => return CheckStatus::Failure,
            "INPROGRESS" | "PENDING" | "RUNNING" | "PAUSED" => any_pending = true,
            _ => {}
        }
    }
    if any_pending {
        CheckStatus::Pending
    } else {
        CheckStatus::Success
    }
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

pub async fn cmd_pr_for_branch(
    state: &AppState,
    project_id: String,
    branch: String,
) -> Result<PrInfoResult, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

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
                            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt,statusCheckRollup,latestReviews,reviews,reviewRequests,comments",
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
                        Ok(Some(mut pr)) => {
                            if let Ok(checks) = run_cli_local(
                                &repo_path,
                                "bkt",
                                &["pr", "checks", pr.number.as_str(), "--json"],
                            ) {
                                pr.check_status = aggregate_bkt_checks(&checks);
                            }
                            Ok(PrInfoResult {
                                pr: Some(pr),
                                provider: Some("bitbucket".to_string()),
                                error: None,
                            })
                        }
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
                            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt,statusCheckRollup,latestReviews,reviews,reviewRequests,comments",
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
                        Ok(Some(mut pr)) => {
                            if let Ok(checks) = run_cli_remote(
                                &state,
                                &project_id,
                                &repo_path,
                                "bkt",
                                &["pr", "checks", pr.number.as_str(), "--json"],
                            )
                            .await
                            {
                                pr.check_status = aggregate_bkt_checks(&checks);
                            }
                            Ok(PrInfoResult {
                                pr: Some(pr),
                                provider: Some("bitbucket".to_string()),
                                error: None,
                            })
                        }
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

pub async fn cmd_pr_list_for_repo(
    state: &AppState,
    project_id: String,
) -> Result<Vec<PrInfo>, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

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
                            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt,statusCheckRollup,latestReviews,reviews,reviewRequests,comments",
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
                            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt,statusCheckRollup,latestReviews,reviews,reviewRequests,comments",
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
#[tauri::command]
pub async fn pr_for_branch(
    project_id: String,
    branch: String,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<PrInfoResult, String> {
    crate::commands::pr_for_branch(state.inner().as_ref(), project_id, branch).await
}

#[tauri::command]
pub async fn pr_threads_for_branch(
    project_id: String,
    pr_number: String,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<PrThreadsResult, String> {
    crate::commands::pr_threads_for_branch(state.inner().as_ref(), project_id, pr_number).await
}

#[tauri::command]
pub async fn pr_list_for_repo(
    project_id: String,
    state: tauri::State<'_, Arc<crate::AppState>>,
) -> Result<Vec<PrInfo>, String> {
    crate::commands::pr_list_for_repo(state.inner().as_ref(), project_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_owner_repo_handles_url_forms() {
        assert_eq!(
            parse_owner_repo("https://github.com/owner/repo.git"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(
            parse_owner_repo("git@github.com:owner/repo.git"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(
            parse_owner_repo("ssh://git@github.com/owner/repo"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(
            parse_owner_repo("https://gitlab.com/owner/repo.git"),
            None
        );
    }

    #[test]
    fn parse_gh_pr_extracts_reviews_requests_and_comments() {
        let json = r#"{
            "number": 35,
            "title": "feat(ui): dialog",
            "url": "https://github.com/owner/repo/pull/35",
            "state": "OPEN",
            "author": {"login": "alice"},
            "headRefName": "feature-x",
            "baseRefName": "main",
            "createdAt": "2026-08-28T09:41:51Z",
            "updatedAt": "2026-08-28T12:11:04Z",
            "statusCheckRollup": [],
            "latestReviews": [
                {"author": {"login": "bob"}, "state": "APPROVED", "submittedAt": "2026-08-28T10:00:00Z"},
                {"author": {"login": "carol"}, "state": "CHANGES_REQUESTED", "submittedAt": "2026-08-28T10:05:00Z"}
            ],
            "reviews": [],
            "reviewRequests": [{"__typename": "User", "login": "dave"}],
            "comments": [
                {"author": {"login": "erin"}, "body": "issue comment", "createdAt": "2026-08-28T11:00:00Z"}
            ]
        }"#;

        let pr = parse_gh_pr(json).unwrap();
        assert_eq!(pr.reviews.len(), 2);
        assert_eq!(pr.reviews[0].author, "bob");
        assert_eq!(pr.reviews[0].state, "approved");
        assert_eq!(pr.reviews[1].state, "changes_requested");
        assert_eq!(pr.review_requests, vec!["dave".to_string()]);
        assert_eq!(pr.comments.len(), 1);
        assert_eq!(pr.comments[0].author, "erin");
        assert!(!pr.comments[0].outdated);
    }

    #[test]
    fn parse_gh_pr_tolerates_missing_review_fields() {
        let json = r#"{
            "number": 1,
            "title": "t",
            "url": "https://github.com/o/r/pull/1",
            "state": "OPEN",
            "headRefName": "a",
            "baseRefName": "b",
            "createdAt": "",
            "updatedAt": ""
        }"#;
        let pr = parse_gh_pr(json).unwrap();
        assert!(pr.reviews.is_empty());
        assert!(pr.review_requests.is_empty());
        assert!(pr.comments.is_empty());
        assert_eq!(pr.viewer_login, None);
    }

    #[test]
    fn parse_gh_threads_marks_outdated_and_viewer_replied() {
        let json = r#"{
            "data": {
                "viewer": {"login": "me"},
                "repository": {"pullRequest": {"reviewThreads": {"nodes": [
                    {
                        "isOutdated": true,
                        "isResolved": false,
                        "comments": {"nodes": [
                            {"databaseId": 7, "author": {"login": "alice"}, "body": "nit", "createdAt": "2026-08-28T10:00:00Z", "path": "src/a.rs"},
                            {"databaseId": 8, "author": {"login": "me"}, "body": "fixed", "createdAt": "2026-08-28T10:01:00Z", "path": "src/a.rs"}
                        ]}
                    },
                    {
                        "isOutdated": false,
                        "isResolved": true,
                        "comments": {"nodes": [
                            {"databaseId": 9, "author": {"login": "bob"}, "body": "ok?", "createdAt": "2026-08-28T09:00:00Z", "path": "src/b.rs"}
                        ]}
                    }
                ]}}}
            }
        }"#;

        let (viewer, comments) = parse_gh_threads(json);
        assert_eq!(viewer.as_deref(), Some("me"));
        assert_eq!(comments.len(), 3);

        // Comments sorted newest first.
        assert_eq!(comments[0].id, "8");
        assert!(comments[0].outdated);
        assert!(comments[0].viewer_replied, "viewer's own comment sits in a thread they replied to");
        assert_eq!(comments[1].id, "7");
        assert!(comments[1].outdated);
        assert!(comments[1].viewer_replied, "viewer replied to this thread");
        assert_eq!(comments[1].file_path.as_deref(), Some("src/a.rs"));
        assert_eq!(comments[2].id, "9");
        assert!(!comments[2].outdated);
        assert!(comments[2].resolved);
        assert!(!comments[2].viewer_replied);
    }

    #[test]
    fn parse_gh_threads_survives_null_author() {
        let json = r#"{
            "data": {
                "viewer": {"login": "me"},
                "repository": {"pullRequest": {"reviewThreads": {"nodes": [
                    {"isOutdated": false, "isResolved": false, "comments": {"nodes": [
                        {"databaseId": 1, "author": null, "body": "ghost", "createdAt": "2026-08-28T10:00:00Z", "path": null}
                    ]}}
                ]}}}
            }
        }"#;
        let (_, comments) = parse_gh_threads(json);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].author, "unknown");
        assert_eq!(comments[0].file_path, None);
    }

    #[test]
    fn parse_gh_pr_list_delegates_to_single_parser() {
        let json = r#"[{
            "number": 2,
            "title": "t",
            "url": "https://github.com/o/r/pull/2",
            "state": "OPEN",
            "author": {"login": "a"},
            "headRefName": "h",
            "baseRefName": "m",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z",
            "statusCheckRollup": []
        }]"#;
        let prs = parse_gh_pr_list(json).unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].number, "2");
        assert!(prs[0].reviews.is_empty());
    }
}
