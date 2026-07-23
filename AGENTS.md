# AGENTS.md

## Project Overview

Agent IDE is a lightweight cross-platform IDE built with Tauri v2 (Rust backend) and React 19 (TypeScript frontend). It supports local and SSH-remote projects, git worktree management, integrated terminals, and Monaco-based code editing.

## Tech Stack

- **Desktop framework:** Tauri v2
- **Frontend:** React 19, TypeScript, Vite
- **Styling:** Tailwind CSS v4 (via `@tailwindcss/vite` plugin), Catppuccin Mocha dark theme
- **Code editor:** Monaco Editor (`@monaco-editor/react`)
- **Terminal:** xterm.js (`@xterm/xterm`) + portable-pty (Rust)
- **SSH:** russh + russh-sftp (Rust)
- **Git:** git2 crate (libgit2 bindings)
- **State management:** Zustand
- **Icons:** Lucide React
- **Layout:** react-resizable-panels

## Project Structure

```
src/                        # Frontend (React + TypeScript)
├── components/
│   ├── layout/             # AppLayout, TitleBar, StatusBar
│   ├── sidebar/            # LeftSidebar (projects/worktrees), RightSidebar (file tree)
│   └── main/               # MainArea, TerminalZone, EditorZone
├── types/                  # TypeScript type definitions
├── App.tsx                 # Root component
├── main.tsx                # Entry point
└── index.css               # Tailwind import + Catppuccin Mocha theme variables

src-tauri/                  # Backend (Rust)
├── src/
│   ├── lib.rs              # Tauri app setup, plugin registration, commands
│   └── main.rs             # Entry point
├── Cargo.toml              # Rust dependencies
├── tauri.conf.json         # Tauri configuration (window, bundle, build)
└── capabilities/           # Permission capabilities (dialog, fs)
```

## Commands

| Command | Purpose |
|---------|---------|
| `npm run tauri dev` | Run full app in dev mode (frontend + Rust backend, hot reload) |
| `npm run dev` | Vite dev server only (no Tauri window) |
| `npm run build` | Build frontend for production (`tsc && vite build`) |
| `npm run tauri build` | Build production desktop app (creates installable bundle) |
| `cargo check` (in `src-tauri/`) | Verify Rust code compiles |
| `npx tsc --noEmit` | Verify TypeScript compiles (typecheck) |

Always run `npx tsc --noEmit` and `cargo check` (in `src-tauri/`) after making changes to verify correctness.

## Code Conventions

### Frontend (React + TypeScript)

- Use functional components with hooks only (no class components)
- Use `export default` for component files
- File naming: PascalCase for components (e.g., `LeftSidebar.tsx`), camelCase for utilities
- Use Tailwind CSS classes for styling — reference theme colors via CSS variables: `bg-[var(--color-base)]`, `text-[var(--color-text)]`, etc.
- Theme palette is defined in `src/index.css` under `@theme` (Catppuccin Mocha)
- State management via Zustand stores — create separate stores per domain
- Types defined in `src/types/index.ts`
- No comments in code unless absolutely necessary

### Backend (Rust)

- Tauri commands defined in `src-tauri/src/lib.rs` (split into modules as the file grows)
- Use `#[tauri::command]` macro for all frontend-callable functions
- Register commands in `tauri::generate_handler![...]` in the `run()` function
- Use `serde` for serialization of all command parameters and return types
- Error handling: return `Result<T, String>` from commands for frontend error display
- Async commands use `#[tauri::command(async)]` with tokio

### IPC (Frontend ↔ Backend)

- Frontend calls backend via `import { invoke } from "@tauri-apps/api/core"`
- Backend streams data to frontend via `app.emit("event_name", payload)`
- Frontend listens to events via `import { listen } from "@tauri-apps/api/event"`

## Architecture Decisions

- **Terminal components must never unmount** — toggle visibility with CSS (`display: none`) instead of conditional rendering. Unmounting destroys xterm.js state and PTY connections.
- **File system operations go through a trait abstraction** — `FileSystemProvider` with `LocalFileSystem` and `SftpFileSystem` implementations, dispatched based on project type.
- **SSH uses russh (pure Rust)** — no Node.js sidecar, no native dependency issues.
- **PTY uses portable-pty** — cross-platform, native to Tauri's Rust core.

## Backend-first Logic Principle

Keep the frontend thin: React components and Zustand stores should primarily handle UI state, presentation, and orchestration of backend commands. Domain logic, I/O, file-system semantics, git operations, SSH lifecycle, and persistence should live in the Rust backend.

### Frontend responsibilities

- UI state (expanded tree nodes, active tabs, dialogs, selection, focus)
- Mapping backend data to React renders
- Calling coarse-grained backend commands and updating local store with returned canonical state
- Presentation helpers that are purely visual (icon colors, breadcrumbs, name formatting)

### Backend responsibilities

- File-system operations and path resolution (already covered by `FileSystemProvider`)
- Gitignore parsing and filtered directory listings
- Sorting, filtering, or aggregating file-system/git data before returning it
- Project CRUD, persistence, and project-lifecycle side effects (SSH connect/disconnect, keychain password storage/deletion)
- Normalizing and validating stored project state (e.g., ensuring only one active worktree)
- Git worktree and branch operations
- SSH connection lifecycle and auto-reconnection
- Terminal process state (cwd resolution, activity/busy inference, process-group tracking)
- Binary-safe or low-overhead IPC payloads for streaming data (e.g., terminal output)

### When implementing a feature

1. Ask whether the logic touches files, git, SSH, processes, secrets, or persisted state. If yes, implement it in Rust.
2. Expose a small, coarse-grained Tauri command rather than multiple fine-grained frontend calls that reconstruct state from raw IPC.
3. Return canonical state from mutating commands so the frontend can replace local store state instead of duplicating validation.
4. Avoid duplicating backend knowledge in the frontend (e.g., path formats, worktree layout, connection schemas, active-state invariants).

### Concrete examples

- **Do not** parse `.gitignore` or filter directory entries in TypeScript. Add filtered listing to the `FileSystemProvider`.
- **Do not** manually sort directory entries after `fs_read_dir`. Let the backend sort, or add a `sort` parameter.
- **Do not** call `save_projects`, `ssh_connect`, and `ssh_store_password` separately from UI code. Use a unified `add_project` command that handles persistence, secret storage, and connection setup.
- **Do not** infer terminal busy state from output bytes and timers. Push process-group or shell-state events from the backend.

## GitHub

- **Repo:** https://github.com/krebbl/agent-ide
- **Project board:** https://github.com/users/krebbl/projects/1
- **Issues:** Labeled by phase (`phase-1-scaffolding` through `phase-9-advanced`) and type (`frontend`, `backend`, `architecture`)
- **Milestones:** One per phase with weekly due dates
- Do not commit or push unless explicitly asked

## Tool Usage Rules

When generating tool calls, you must only call the explicitly provided tools. Do not output empty tool syntax, placeholder fields, or any tool formatting blocks outside of the official tools listed. Do not output an 'unknown' tool call.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **agent-ide** (1442 symbols, 2640 relationships, 125 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/agent-ide/context` | Codebase overview, check index freshness |
| `gitnexus://repo/agent-ide/clusters` | All functional areas |
| `gitnexus://repo/agent-ide/processes` | All execution flows |
| `gitnexus://repo/agent-ide/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
