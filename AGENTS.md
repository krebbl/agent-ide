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

## GitHub

- **Repo:** https://github.com/krebbl/agent-ide
- **Project board:** https://github.com/users/krebbl/projects/1
- **Issues:** Labeled by phase (`phase-1-scaffolding` through `phase-9-advanced`) and type (`frontend`, `backend`, `architecture`)
- **Milestones:** One per phase with weekly due dates
- Do not commit or push unless explicitly asked
