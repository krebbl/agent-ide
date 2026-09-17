use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

use crate::{AppState, JiraProjectConfig, Project};

const TOKEN_PREFIX: &str = "jira_api_token:";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraComment {
    pub author: String,
    pub created: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub issue_type: String,
    pub priority: Option<String>,
    pub assignee: Option<String>,
    pub reporter: Option<String>,
    pub url: String,
    pub labels: Vec<String>,
    pub created: String,
    pub updated: String,
    pub comments: Vec<JiraComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueResult {
    pub issue: Option<JiraIssue>,
    pub ticket_key: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraConfigState {
    pub site_url: String,
    pub email: String,
    pub has_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraConfigUpdate {
    pub config: JiraConfigState,
    pub project: Project,
}

fn token_key(project_id: &str) -> String {
    format!("{}{}", TOKEN_PREFIX, project_id)
}

fn config_state(project: &Project) -> Result<JiraConfigState, String> {
    let has_token = crate::secrets::get_secret(&token_key(&project.id))?
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false);
    Ok(JiraConfigState {
        site_url: project.jira_config.as_ref().map(|c| c.site_url.clone()).unwrap_or_default(),
        email: project.jira_config.as_ref().map(|c| c.email.clone()).unwrap_or_default(),
        has_token,
    })
}

fn normalize_site_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

// ── Ticket key extraction ────────────────────────────────────────────────────

fn extract_issue_key(branch: &str) -> Option<String> {
    let bytes = branch.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_uppercase() {
            i += 1;
            continue;
        }
        if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < bytes.len() && (bytes[end].is_ascii_uppercase() || bytes[end].is_ascii_digit()) {
            end += 1;
        }
        let left_len = end - i;
        if left_len >= 2
            && bytes[end - 1].is_ascii_uppercase()
            && end < bytes.len()
            && bytes[end] == b'-'
        {
            let mut d = end + 1;
            while d < bytes.len() && bytes[d].is_ascii_digit() {
                d += 1;
            }
            if d > end + 1 && (d >= bytes.len() || !bytes[d].is_ascii_alphanumeric()) {
                return Some(branch[i..d].to_string());
            }
        }
        i += 1;
    }
    None
}

// ── Jira Cloud REST API ──────────────────────────────────────────────────────

fn json_str(value: &serde_json::Value, pointer: &str) -> Option<String> {
    value.pointer(pointer).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn user_display_name(user: Option<&serde_json::Value>) -> Option<String> {
    user.and_then(|u| u.get("displayName"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string())
}

fn parse_issue(key: &str, site_url: &str, payload: &serde_json::Value) -> Result<JiraIssue, String> {
    let fields = payload
        .get("fields")
        .ok_or_else(|| format!("Unexpected Jira response for {}", key))?;

    let empty = Vec::new();
    let comments = fields
        .pointer("/comment/comments")
        .and_then(|c| c.as_array())
        .unwrap_or(&empty)
        .iter()
        .map(|c| JiraComment {
            author: json_str(c, "/author/displayName").unwrap_or_else(|| "Unknown".to_string()),
            created: json_str(c, "/created").unwrap_or_default(),
            body: json_str(c, "/body").unwrap_or_default(),
        })
        .collect();

    let labels = fields
        .get("labels")
        .and_then(|l| l.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(JiraIssue {
        key: key.to_string(),
        summary: json_str(fields, "/summary").unwrap_or_default(),
        description: json_str(fields, "/description").filter(|s| !s.trim().is_empty()),
        status: json_str(fields, "/status/name").unwrap_or_else(|| "Unknown".to_string()),
        issue_type: json_str(fields, "/issuetype/name").unwrap_or_else(|| "Unknown".to_string()),
        priority: json_str(fields, "/priority/name"),
        assignee: user_display_name(fields.get("assignee")),
        reporter: user_display_name(fields.get("reporter")),
        url: format!("{}/browse/{}", site_url, key),
        labels,
        created: json_str(fields, "/created").unwrap_or_default(),
        updated: json_str(fields, "/updated").unwrap_or_default(),
        comments,
    })
}

async fn fetch_issue(
    site_url: &str,
    email: &str,
    token: &str,
    key: &str,
) -> Result<JiraIssue, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let url = format!(
        "{}/rest/api/2/issue/{}?fields=summary,description,status,issuetype,priority,assignee,reporter,labels,created,updated,comment",
        site_url, key
    );
    let resp = client
        .get(&url)
        .basic_auth(email, Some(token))
        .send()
        .await
        .map_err(|e| format!("Jira request failed: {}", e))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err("Jira rejected the credentials (401/403). Check site URL, email and API token.".to_string());
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("Ticket {} not found on {}", key, site_url));
    }
    if !status.is_success() {
        return Err(format!("Jira returned HTTP {} for {}", status, key));
    }

    let payload: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse Jira response: {}", e))?;
    parse_issue(key, site_url, &payload)
}

// ── Commands ─────────────────────────────────────────────────────────────────

pub async fn cmd_jira_get_config(state: &AppState, project_id: String) -> Result<JiraConfigState, String> {
    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;
    config_state(project)
}

pub async fn cmd_jira_set_config(
    state: &AppState,
    project_id: String,
    site_url: String,
    email: String,
    api_token: Option<String>,
) -> Result<JiraConfigUpdate, String> {
    let mut projects = crate::commands::load_projects(state).await?;
    let (config_state, project) = {
        let project = projects
            .iter_mut()
            .find(|p| p.id == project_id)
            .ok_or("Project not found")?;

        let site_url = normalize_site_url(&site_url);
        if !site_url.is_empty() && !site_url.starts_with("http://") && !site_url.starts_with("https://") {
            return Err("Site URL must start with http:// or https://".to_string());
        }
        let email = email.trim().to_string();
        if let Some(token) = api_token.as_deref() {
            if !token.trim().is_empty() {
                crate::secrets::set_secret(&token_key(&project_id), token.trim())?;
            }
        }
        project.jira_config = if site_url.is_empty() && email.is_empty() {
            None
        } else {
            Some(JiraProjectConfig {
                site_url,
                email,
            })
        };
        let cfg = config_state(project)?;
        (cfg, project.clone())
    };

    crate::commands::save_projects(state, projects).await?;
    Ok(JiraConfigUpdate {
        config: config_state,
        project,
    })
}

pub async fn cmd_jira_issue_for_branch(
    state: &AppState,
    project_id: String,
    branch: String,
) -> Result<JiraIssueResult, String> {
    let ticket_key = extract_issue_key(&branch);
    let Some(ticket_key) = ticket_key else {
        return Ok(JiraIssueResult {
            issue: None,
            ticket_key: None,
            error: Some(format!("No Jira ticket key found in branch '{}'", branch)),
        });
    };

    let projects = crate::commands::load_projects(state).await?;
    let project = projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    let config = project
        .jira_config
        .as_ref()
        .filter(|c| !c.site_url.trim().is_empty() && !c.email.trim().is_empty());
    let token = crate::secrets::get_secret(&token_key(&project_id))?.filter(|t| !t.trim().is_empty());
    let (Some(config), Some(token)) = (config, token) else {
        return Ok(JiraIssueResult {
            issue: None,
            ticket_key: Some(ticket_key),
            error: Some("Jira is not configured for this project. Set site URL, email and API token via the gear icon.".to_string()),
        });
    };

    match fetch_issue(&config.site_url, &config.email, &token, &ticket_key).await {
        Ok(issue) => Ok(JiraIssueResult {
            issue: Some(issue),
            ticket_key: Some(ticket_key),
            error: None,
        }),
        Err(e) => Ok(JiraIssueResult {
            issue: None,
            ticket_key: Some(ticket_key),
            error: Some(e),
        }),
    }
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn jira_get_config(
    project_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<JiraConfigState, String> {
    cmd_jira_get_config(state.inner().as_ref(), project_id).await
}

#[tauri::command]
pub async fn jira_set_config(
    project_id: String,
    site_url: String,
    email: String,
    api_token: Option<String>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<JiraConfigUpdate, String> {
    cmd_jira_set_config(state.inner().as_ref(), project_id, site_url, email, api_token).await
}

#[tauri::command]
pub async fn jira_issue_for_branch(
    project_id: String,
    branch: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<JiraIssueResult, String> {
    cmd_jira_issue_for_branch(state.inner().as_ref(), project_id, branch).await
}

#[cfg(test)]
mod tests {
    use super::extract_issue_key;

    #[test]
    fn extracts_key_from_branch() {
        assert_eq!(extract_issue_key("feature/PROJ-123-add-search"), Some("PROJ-123".to_string()));
        assert_eq!(extract_issue_key("proj/AB-4_fix"), Some("AB-4".to_string()));
        assert_eq!(extract_issue_key("PROJ-1"), Some("PROJ-1".to_string()));
    }

    #[test]
    fn rejects_non_ticket_branches() {
        assert_eq!(extract_issue_key("main"), None);
        assert_eq!(extract_issue_key("feature/add-search"), None);
        assert_eq!(extract_issue_key("release/v1.2"), None);
        assert_eq!(extract_issue_key("a-1"), None);
        assert_eq!(extract_issue_key("sha256-123abc"), None);
    }
}
