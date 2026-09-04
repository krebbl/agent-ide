use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::{AppState, Project, WorktreeTabs};

pub use crate::{
    cmd_build_agent_command as build_agent_command,
    cmd_check_agent_ready as check_agent_ready,
    cmd_check_agents_ready as check_agents_ready,
    cmd_check_is_git_repo as check_is_git_repo,
    cmd_fs_exists as fs_exists,
    cmd_fs_mkdir as fs_mkdir,
    cmd_fs_mv as fs_mv,
    cmd_fs_read_dir as fs_read_dir,
    cmd_fs_read_file as fs_read_file,
    cmd_fs_rm as fs_rm,
    cmd_fs_search_files as fs_search_files,
    cmd_fs_stat as fs_stat,
    cmd_fs_write_file as fs_write_file,
    cmd_git_branches_available_for_worktrees_async as git_branches_available_for_worktrees_async,
    cmd_git_branches_list_async as git_branches_list_async,
    cmd_git_init as git_init,
    cmd_git_worktree_add_async as git_worktree_add_async,
    cmd_git_worktree_list as git_worktree_list,
    cmd_git_worktree_list_async as git_worktree_list_async,
    cmd_git_worktree_remove_async as git_worktree_remove_async,
    cmd_load_editor_tabs as load_editor_tabs,
    cmd_load_expanded_projects as load_expanded_projects,
    cmd_load_projects as load_projects,
    cmd_list_agent_models as list_agent_models,
    cmd_list_local_dir as list_local_dir,
    cmd_save_editor_tabs as save_editor_tabs,
    cmd_save_expanded_projects as save_expanded_projects,
    cmd_save_projects as save_projects,
    cmd_ssh_check_git as ssh_check_git,
    cmd_ssh_connect as ssh_connect,
    cmd_ssh_delete_password as ssh_delete_password,
    cmd_ssh_disconnect as ssh_disconnect,
    cmd_ssh_get_password as ssh_get_password,
    cmd_ssh_list_directory as ssh_list_directory,
    cmd_ssh_store_password as ssh_store_password,
    cmd_ssh_test_connection as ssh_test_connection,
    cmd_util_open_url as util_open_url,
};

pub use crate::lsp::{
    cmd_lsp_list as lsp_list,
    cmd_lsp_notify as lsp_notify,
    cmd_lsp_request as lsp_request,
    cmd_lsp_server_available as lsp_server_available,
    cmd_lsp_start as lsp_start,
    cmd_lsp_stop as lsp_stop,
};

pub use crate::notification::{cmd_notification_show as notification_show};

pub use crate::pty::{
    cmd_pty_kill as pty_kill,
    cmd_pty_list_sessions as pty_list_sessions,
    cmd_pty_nudge as pty_nudge,
    cmd_pty_register_ssh_project as pty_register_ssh_project,
    cmd_pty_resize as pty_resize,
    cmd_pty_session_processes as pty_session_processes,
    cmd_pty_set_active as pty_set_active,
    cmd_pty_spawn as pty_spawn,
    cmd_pty_write as pty_write,
};

pub use crate::pr_info::{cmd_pr_for_branch as pr_for_branch, cmd_pr_list_for_repo as pr_list_for_repo};

pub use crate::cmd_ssh_agent_info as ssh_agent_info;

pub async fn dispatch(
    state: &AppState,
    command: &str,
    payload: Value,
) -> Result<Value, String> {
    macro_rules! cmd_state {
        ($req:ident, $fn:path, [$($field:ident: $ty:ty),*$(,)?]) => {{
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct $req { $($field: $ty),* }
            let req: $req = serde_json::from_value(payload).map_err(|e| e.to_string())?;
            let res = $fn(state, $(req.$field),*).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }};
    }

    macro_rules! cmd_plain {
        ($req:ident, $fn:path, [$($field:ident: $ty:ty),*$(,)?]) => {{
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct $req { $($field: $ty),* }
            let req: $req = serde_json::from_value(payload).map_err(|e| e.to_string())?;
            let res = $fn($(req.$field),*).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }};
    }

    match command {
        "util_open_url" => cmd_plain!(UtilOpenUrlReq, util_open_url, [url: String]),
        "fs_read_dir" => cmd_state!(FsReadDirReq, fs_read_dir, [project_id: String, path: String]),
        "fs_read_file" => cmd_state!(FsReadFileReq, fs_read_file, [project_id: String, path: String]),
        "fs_write_file" => cmd_state!(FsWriteFileReq, fs_write_file, [project_id: String, path: String, content: String]),
        "fs_stat" => cmd_state!(FsStatReq, fs_stat, [project_id: String, path: String]),
        "fs_mkdir" => cmd_state!(FsMkdirReq, fs_mkdir, [project_id: String, path: String]),
        "fs_rm" => cmd_state!(FsRmReq, fs_rm, [project_id: String, path: String, recursive: Option<bool>]),
        "fs_mv" => cmd_state!(FsMvReq, fs_mv, [project_id: String, from: String, to: String]),
        "fs_exists" => cmd_state!(FsExistsReq, fs_exists, [project_id: String, path: String]),
        "list_local_dir" => cmd_plain!(ListLocalDirReq, list_local_dir, [path: String]),
        "fs_search_files" => cmd_state!(FsSearchFilesReq, fs_search_files, [project_id: String, root: String, query: String, limit: Option<usize>]),
        "check_agent_ready" => cmd_plain!(CheckAgentReadyReq, check_agent_ready, [id: String]),
        "check_agents_ready" => {
            let res = check_agents_ready().await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "list_agent_models" => cmd_plain!(ListAgentModelsReq, list_agent_models, [id: String]),
        "build_agent_command" => cmd_plain!(BuildAgentCommandReq, build_agent_command, [agent_id: String, model: Option<String>, prompt: String]),
        "save_projects" => cmd_state!(SaveProjectsReq, save_projects, [projects: Vec<Project>]),
        "load_projects" => {
            let res = load_projects(state).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "save_expanded_projects" => cmd_state!(SaveExpandedProjectsReq, save_expanded_projects, [ids: Vec<String>]),
        "load_expanded_projects" => {
            let res = load_expanded_projects(state).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "save_editor_tabs" => cmd_state!(SaveEditorTabsReq, save_editor_tabs, [tabs: HashMap<String, WorktreeTabs>]),
        "load_editor_tabs" => {
            let res = load_editor_tabs(state).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "check_is_git_repo" => cmd_plain!(CheckIsGitRepoReq, check_is_git_repo, [path: String]),
        "git_init" => cmd_plain!(GitInitReq, git_init, [path: String]),
        "git_worktree_list" => cmd_state!(GitWorktreeListReq, git_worktree_list, [project_id: String]),
        "git_worktree_list_async" => cmd_state!(GitWorktreeListAsyncReq, git_worktree_list_async, [project_id: String]),
        "git_worktree_add_async" => cmd_state!(GitWorktreeAddAsyncReq, git_worktree_add_async, [project_id: String, branch: String, name: String, new_branch: Option<bool>, base_branch: Option<String>, command: Option<String>]),
        "git_worktree_remove_async" => cmd_state!(GitWorktreeRemoveAsyncReq, git_worktree_remove_async, [project_id: String, worktree_path: String, force: Option<bool>, delete_branch: Option<bool>]),
        "git_branches_list_async" => cmd_state!(GitBranchesListAsyncReq, git_branches_list_async, [project_id: String]),
        "git_branches_available_for_worktrees_async" => cmd_state!(GitBranchesAvailableForWorktreesAsyncReq, git_branches_available_for_worktrees_async, [project_id: String]),
        "ssh_agent_info" => {
            let res = ssh_agent_info().await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "ssh_test_connection" => cmd_plain!(SshTestConnectionReq, ssh_test_connection, [host: String, port: u16, username: String, auth_method: String, key_path: Option<String>, password: Option<String>]),
        "ssh_connect" => cmd_state!(SshConnectReq, ssh_connect, [project_id: String, host: String, port: u16, username: String, auth_method: String, key_path: Option<String>, password: Option<String>]),
        "ssh_disconnect" => cmd_state!(SshDisconnectReq, ssh_disconnect, [project_id: String]),
        "ssh_list_directory" => cmd_state!(SshListDirectoryReq, ssh_list_directory, [project_id: String, path: String]),
        "ssh_check_git" => cmd_state!(SshCheckGitReq, ssh_check_git, [project_id: String, path: String]),
        "ssh_store_password" => cmd_plain!(SshStorePasswordReq, ssh_store_password, [project_id: String, password: String]),
        "ssh_get_password" => cmd_plain!(SshGetPasswordReq, ssh_get_password, [project_id: String]),
        "ssh_delete_password" => cmd_plain!(SshDeletePasswordReq, ssh_delete_password, [project_id: String]),
        "notification_show" => cmd_plain!(NotificationShowReq, notification_show, [title: String, body: String, session_id: Option<String>]),
        "pty_spawn" => cmd_state!(PtySpawnReq, pty_spawn, [cwd: Option<String>, cols: u16, rows: u16, project_id: Option<String>, worktree_id: Option<String>, session_type: Option<String>, argv: Option<Vec<String>>]),
        "pty_list_sessions" => {
            let res = pty_list_sessions(state).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "pty_write" => cmd_state!(PtyWriteReq, pty_write, [session_id: String, data: String]),
        "pty_resize" => cmd_state!(PtyResizeReq, pty_resize, [session_id: String, cols: u16, rows: u16]),
        "pty_nudge" => cmd_state!(PtyNudgeReq, pty_nudge, [session_id: String]),
        "pty_kill" => cmd_state!(PtyKillReq, pty_kill, [session_id: String]),
        "pty_session_processes" => cmd_state!(PtySessionProcessesReq, pty_session_processes, [session_id: String]),
        "pty_set_active" => cmd_state!(PtySetActiveReq, pty_set_active, [pty_id: Option<String>]),
        "pty_register_ssh_project" => cmd_state!(PtyRegisterSshProjectReq, pty_register_ssh_project, [project_id: String, host: String, port: u16, username: String, auth_method: String, key_path: Option<String>, password: Option<String>]),
        "pr_for_branch" => cmd_state!(PrForBranchReq, pr_for_branch, [project_id: String, branch: String]),
        "pr_list_for_repo" => cmd_state!(PrListForRepoReq, pr_list_for_repo, [project_id: String]),
        "lsp_start" => cmd_state!(LspStartReq, lsp_start, [project_id: String, language_id: String, root_path: String]),
        "lsp_request" => cmd_state!(LspRequestReq, lsp_request, [project_id: String, language_id: String, method: String, params: Value]),
        "lsp_notify" => cmd_state!(LspNotifyReq, lsp_notify, [project_id: String, language_id: String, method: String, params: Value]),
        "lsp_stop" => cmd_state!(LspStopReq, lsp_stop, [project_id: String, language_id: String]),
        "lsp_list" => {
            let res = lsp_list(state).await?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "lsp_server_available" => cmd_state!(LspServerAvailableReq, lsp_server_available, [project_id: String, language_id: String]),
        _ => Err(format!("Unknown command: {}", command)),
    }
}
