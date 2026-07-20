# Agent Hook Notifications — Implementation Plan

## Goal

Detect when an AI coding agent (Claude, Codex, etc.) finishes a prompt / task turn inside an agent-ide terminal, and surface that as a frontend event so the UI can show agent activity per session.

## Approach

Project-local hooks: install agent wrapper scripts and hook configuration inside each project directory. Every terminal spawned for that project knows where the hooks live via environment variables. When an agent fires a hook, it appends a JSON line to a project-local events file; the Rust backend polls that file and emits a Tauri event to the frontend.

## Why project-local

- No global `~/.claude/settings.json` pollution.
- One install per project, shared across all worktrees of that project.
- Remote projects work the same way once hooks are uploaded via SFTP.
- Keeps agent configuration close to the code the agent operates on.

## Backend

### `src-tauri/src/agents.rs`

Keep the existing agent catalog and readiness check functions:

- `builtin_agents()`
- `check_agent_ready(id)`
- `check_all_agents_ready()`
- `find_real_binary(name)` — resolves real executable on PATH, excludes wrapper dirs.

Add project-local hook helpers:

- `project_hooks_dir(project_path)`
- `project_events_dir(project_path)`
- `project_bin_dir(project_path)`
- `ensure_agent_hooks_local(project_path)` — writes wrappers, notify script, merged `.claude/settings.json`.
- `ensure_agent_hooks_remote(sftp, project_path)` — SFTP upload version.

Wrapper script responsibilities:

- Export `AGENT_IDE_AGENT=<binary-name>`.
- Export `AGENT_IDE_EVENTS_DIR=<project>/.agent-ide/events`.
- Resolve and `exec` the real agent binary (skip the project `.agent-ide/bin` dir).

Notify script responsibilities:

- Read `AGENT_IDE_TAB_ID` and `AGENT_IDE_EVENTS_DIR` from environment.
- Append one JSON line per event to `<events-dir>/<tab-id>.jsonl`.

Claude `settings.json` responsibilities:

- Merge into existing `.claude/settings.json` instead of overwriting `Stop` / `PermissionRequest`.
- Add `Stop` and `PermissionRequest` command hooks that call the project-local notify script.
- Set `env.AGENT_IDE_EVENTS_DIR` so Claude passes it to hook children.

### `src-tauri/src/agent_events.rs`

`AgentEventPoller`:

- Registers local and remote sessions.
- Polls every 500ms (or less aggressively).
- Tracks last-read byte offset per session.
- Reads local files with `tokio::fs` and remote files via SFTP.
- Emits `agent:event` Tauri event with `{ sessionId, event, agent?, data? }`.

Keep remote SFTP reads lightweight; consider checking file metadata (size/mtime) before opening to reduce round trips.

### `src-tauri/src/lib.rs`

- `mod agent_events;`
- Add `agent_event_poller` field to `AppState`.
- Start the poller during `setup`.
- Add `ensure_agent_hooks(project_dir)` command.
- Call `agents::ensure_agent_hooks_local` inside `save_projects` and `load_projects` for every local project.
- Call `agents::ensure_agent_hooks_remote` inside `ssh_connect` after SFTP is available.

### `src-tauri/src/pty.rs`

- `pty_spawn` accepts `project_path: Option<String>`.
- `PtyManager::spawn` passes project path down.
- `spawn_local` injects:
  - `AGENT_IDE_EVENTS_DIR=<project>/.agent-ide/events`
  - `AGENT_IDE_TAB_ID=<session-id>`
  - `PATH=<project>/.agent-ide/bin:<original PATH>`
- Registers the session with `agent_event_poller`.
- `spawn_remote` sends shell setup commands to create the events dir and export env vars, and registers the remote tracker.
- Unregister sessions on kill/exit.

## Frontend

### Types (`src/types/index.ts`)

```ts
export interface AgentEvent {
  sessionId: string;
  event: string;
  agent?: string;
  data?: unknown;
}

export interface AgentSessionState {
  sessionId: string;
  agent?: string;
  lastEvent: string;
  lastEventAt: number;
  isRunning: boolean;
}
```

### Service (`src/services/agentEvents.ts`)

Listens to `agent:event` and forwards to registered handlers.

### Store (`src/stores/agentStore.ts`)

Zustand store:

- `checkAll()` / `checkById()` for readiness.
- `sessions: Record<string, AgentSessionState>`.
- Updates session state on each `agent:event`.

### Init (`src/main.tsx`)

```ts
initAgentEventListeners().catch(() => {});
```

### Stores

- `projectStore.ts` passes `projectPath` to `ssh_connect`.
- `terminalStore.ts` passes `projectPath` to `pty_spawn`.

## Vite HMR

Because the agent-ide repo itself was opened as a project, writing hooks into the repo triggered Vite HMR reloads. Add ignores in `vite.config.ts`:

```ts
watch: {
  ignored: ["**/src-tauri/**", "**/.agent-ide/**", "**/.claude/**"],
},
```

Also make hook writes idempotent (skip if content unchanged) to avoid unnecessary filesystem churn.

## Testing

1. Add/connect a project.
2. Verify `<project>/.agent-ide/hooks/notify.sh` and `<project>/.claude/settings.json` exist.
3. Open a terminal for the project.
4. Check `env | grep AGENT_IDE` shows `EVENTS_DIR` and `TAB_ID`.
5. Manually trigger:
   ```bash
   <project>/.agent-ide/hooks/notify.sh Stop claude
   ```
6. Verify `<project>/.agent-ide/events/<tab-id>.jsonl` has a JSON line.
7. Frontend should receive `agent:event` with the tab id.
8. Full test: run `claude` from project root, submit a prompt, check `Stop` event arrives.

## Known Limitations / Next Steps

- Only Claude has a defined hook config. Other agents need their own hook mechanisms or wrappers.
- Remote sessions need POSIX shell for the setup commands.
- SFTP remote reads happen every polling interval; optimize with metadata checks.
- Merging into existing `.claude/settings.json` should append to arrays rather than replace to preserve user hooks.
- Wrapper scripts are Unix-only; Windows agents would need separate handling.
