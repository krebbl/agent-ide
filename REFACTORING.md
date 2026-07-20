# Refactoring Plan: Move Frontend Logic to Backend

This document captures concrete refactoring suggestions to move domain logic from the React/TypeScript frontend into the Tauri Rust backend, aligning with the **Backend-first Logic Principle** in `AGENTS.md`.

## 1. Gitignore parsing & directory entry filtering

**Current frontend code:** `src/stores/fileTreeStore.ts` — `loadGitignorePatterns()`, `gitignoreMatch()`, `fetchDirEntries()`

**Issue:**
- The frontend reads `.gitignore`, parses lines, converts globs to regexes, and filters entries returned by `fs_read_dir`.
- This causes an extra IPC round-trip and duplicates work the backend is already doing (sorting).

**Suggested backend change:**
- Add a filtered read command, e.g. `fs_read_dir_filtered(project_id, path, respect_gitignore: bool)`.
- Implement filtering inside the existing `FileSystemProvider` implementations (`LocalFileSystem` and `SftpFileSystem`).

**Frontend cleanup:**
- Remove `loadGitignorePatterns()` and `gitignoreMatch()`.
- Simplify `fileTreeStore.ts` to toggle a `showIgnored` flag and call the new command.

---

## 2. Directory entry sorting

**Current frontend code:** `src/stores/fileTreeStore.ts` — `toggleDir()`, `refreshDir()`, `setShowIgnored()`

**Issue:**
- Each directory fetch manually sorts entries in three places.
- `LocalFileSystem::read_dir` and `SftpFileSystem::read_dir` already sort identically in Rust.

**Suggested backend change:**
- Ensure both providers always return entries sorted (dirs first, then alphabetically).
- Optionally add a `sort: bool` parameter to `fs_read_dir`.

**Frontend cleanup:**
- Remove `.sort(...)` calls in `fileTreeStore.ts`.

---

## 3. Project CRUD lifecycle & persistence side effects

**Current frontend code:** `src/stores/projectStore.ts` — `addProject()`, `removeProject()`, `updateProject()`, `setActiveProject()`, `setActiveWorktree()`

**Issue:**
- Every mutating action re-fetches projects from disk if the store is empty, mutates the array, calls `save_projects`, and triggers SSH side effects manually.
- This is duplicated, non-transactional orchestration that can race.

**Suggested backend commands:**
- `add_project(project)` — persists, handles SSH password storage, triggers `ssh_connect`, returns canonical list.
- `remove_project(id)` — removes project, disconnects SSH, deletes stored password, returns canonical list.
- `update_project(id, updates)` — mutates and persists a single project, returns canonical list.
- `set_active_project(id)` — clears active worktree for all other projects, persists, returns canonical list.
- `set_active_worktree(project_id, worktree_id)` — same as above but scoped to worktrees.

**Frontend cleanup:**
- Replace the orchestration logic in `projectStore.ts` with thin wrappers that emit the command and replace local state with the returned list.

---

## 4. Active-worktree deduplication & type normalization

**Current frontend code:** `src/stores/projectStore.ts` — `loadProjects()`, `loadProjectsFromDisk()`

**Issue:**
- `loadProjects()` enforces that only one project can have an `activeWorktreeId`, then re-saves the corrected list.
- `loadProjectsFromDisk()` casts the serialized connection object because the backend schema differs from the frontend `Project` type.

**Suggested backend change:**
- Move the single-active-worktree validation into `load_projects`.
- Return a fully typed, normalized `Project` from the backend so the frontend never needs to cast.

**Frontend cleanup:**
- Remove the deduplication loop and type casting in `projectStore.ts`.

---

## 5. SSH auto-connect on project load/add

**Current frontend code:** `src/stores/projectStore.ts` — `loadProjects()`, `addProject()`

**Issue:**
- Both functions extract SSH credentials and call `ssh_connect` explicitly.
- Duplicates credential-extraction logic and couples the frontend to SSH lifecycle.

**Suggested backend change:**
- Auto-connect inside `load_projects` / `add_project` when the project type is SSH.
- Continue emitting `ssh_connection_status` events.

**Frontend cleanup:**
- Remove manual `ssh_connect` calls from `projectStore.ts`.
- Keep only the event listener that updates connection status UI.

---

## 6. SSH password/keychain side effects

**Current frontend code:** `src/components/dialogs/AddProjectDialog.tsx`, `src/stores/projectStore.ts`

**Issue:**
- `AddProjectDialog` calls `ssh_store_password` separately before `addProject`.
- `removeProject` calls `ssh_delete_password` separately.

**Suggested backend change:**
- Handle password/keychain entries as part of the project lifecycle in Rust.
- `add_project` stores the password when present.
- `remove_project` deletes the stored password.

**Frontend cleanup:**
- Remove explicit `ssh_store_password` / `ssh_delete_password` calls.

---

## 7. Terminal CWD/title resolution

**Current frontend code:** `src/stores/terminalStore.ts` — `findWorktreePath()`, `resolveCwd()`, `basename()`

**Issue:**
- Frontend resolves the shell working directory from `projectId`/`worktreeId` by scanning project state.
- `pty_spawn` already receives `project_id` and `session_type`.

**Suggested backend change:**
- Extend `pty_spawn` to accept `project_id` and `worktree_id` and resolve the CWD in Rust.

**Frontend cleanup:**
- Remove `findWorktreePath()` and `resolveCwd()` from `terminalStore.ts`.
- Pass identifiers instead of computed paths.

---

## 8. Terminal activity/busy inference

**Current frontend code:** `src/components/main/TerminalView.tsx` — `markBusy()`

**Issue:**
- Busy state is inferred from output bytes using a 1.5s debounce timer.
- A silent long-running process (e.g., `sleep 10`) is incorrectly reported as waiting for input.

**Suggested backend change:**
- Track the foreground process group locally using `portable-pty` child handles.
- On Unix, use `tcgetpgrp`/`kill(pid, 0)`-style checks to determine if a process is running.
- Emit a `pty_activity_changed` event with `isBusy` / `needsInput`.

**Frontend cleanup:**
- Remove output-based `markBusy()` and timers.
- React to `pty_activity_changed` events.

---

## 9. Terminal output encoding

**Current frontend code:** `src-tauri/src/pty.rs`, `src/services/terminalEvents.ts`

**Issue:**
- PTY output is base64-encoded in Rust and decoded in JavaScript with a manual loop.

**Suggested backend/IPC change:**
- Investigate passing `Vec<u8>` directly or using a binary channel (e.g., `tauri::ipc::Channel`) so the frontend receives a `Uint8Array` without base64 overhead.

**Frontend cleanup:**
- Remove `base64ToUint8Array()` from `terminalEvents.ts`.

---

## 10. Remote directory browser filtering

**Current frontend code:** `src/components/dialogs/RemoteDirBrowser.tsx` — `loadDirectory()`

**Issue:**
- Calls `ssh_list_directory`, then filters to directories and sorts in JS.

**Suggested backend change:**
- Add `ssh_list_directories(project_id, path)` that returns only directory names, already sorted.

**Frontend cleanup:**
- Remove client-side filter/sort in `RemoteDirBrowser.tsx`.

---

## What should remain in the frontend

Keep these concerns in the TypeScript/React layer:

- UI state (expanded file-tree nodes, active tabs, dialog visibility, selection, focus)
- Mapping backend data to React renders
- Calling coarse-grained backend commands and replacing local store state with returned canonical state
- Purely visual presentation helpers (file icon colors, breadcrumbs, name formatting)
- Layout filtering (e.g., terminal tabs scoped to active project/worktree)
- Environment-specific helpers (e.g., dev notification fallback)

---

## Priority order

1. **Gitignore filtering & directory sorting** — small, self-contained, removes duplicated logic in the most active store.
2. **Project CRUD lifecycle** — highest architectural impact; makes the backend the single source of truth.
3. **SSH auto-connect + password lifecycle** — reduces duplication and keeps secrets out of the UI layer.
4. **Active-worktree normalization** — data integrity should live in Rust.
5. **Terminal CWD resolution & activity inference** — better correctness and less frontend state.
6. **Binary terminal output** — performance improvement, can be done later.
7. **Remote directory listing filter** — minor cleanup.
