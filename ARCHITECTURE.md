# Agent IDE - Architecture & Task Breakdown

## Overview

A lightweight, cross-platform IDE for coding projects with support for local and SSH-remote projects, git worktree management, integrated terminals, file tree exploration, and Monaco-based code editing.

---

## Recommended Architecture

### Framework: Tauri v2

Tauri v2 is the clear choice for a new desktop IDE in 2026:

| Metric              | Tauri v2       | Electron        |
|---------------------|----------------|-----------------|
| Bundle size         | 5-10 MB        | 80-200 MB       |
| Idle memory         | ~30-50 MB      | ~120-400 MB     |
| Security surface    | Small (Rust)   | Large (Chromium)|
| Mobile support      | Yes (iOS/Android) | No           |
| IPC performance     | Fast (Rust)    | Slower (Node)   |

Tauri uses the OS-native WebView (WebView2/WKWebView/WebKitGTK) and a Rust backend, giving us native performance for file I/O, PTY management, and SSH — all critical for an IDE.

### Technology Stack

```
┌─────────────────────────────────────────────────────────┐
│                    Tauri v2 App Shell                     │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │            Frontend (WebView)                      │   │
│  │  React 19 + TypeScript + Vite + Tailwind CSS     │   │
│  │                                                    │   │
│  │  ┌──────────┐  ┌─────────────┐  ┌──────────────┐ │   │
│  │  │ Left     │  │ Main Area   │  │ Right        │ │   │
│  │  │ Sidebar  │  │             │  │ Sidebar      │ │   │
│  │  │          │  │ Terminal    │  │              │ │   │
│  │  │ Projects │  │ Tabs        │  │ File Tree    │ │   │
│  │  │ +        │  │ (xterm.js)  │  │ Explorer     │ │   │
│  │  │ Worktrees│  │             │  │              │ │   │
│  │  │          │  │ Editor Tabs │  │              │ │   │
│  │  │          │  │ (Monaco)    │  │              │ │   │
│  │  └──────────┘  └─────────────┘  └──────────────┘ │   │
│  └──────────────────────────────────────────────────┘   │
│                          │                               │
│                    Tauri IPC (invoke / events)            │
│                          │                               │
│  ┌──────────────────────────────────────────────────┐   │
│  │            Backend (Rust)                          │   │
│  │                                                    │   │
│  │  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌─────────┐ │   │
│  │  │ File    │ │ PTY      │ │ SSH    │ │ Git     │ │   │
│  │  │ System  │ │ Manager  │ │ Client │ │ Ops     │ │   │
│  │  │ (local) │ │ (portable│ │ (russh)│ │ (git2 / │ │   │
│  │  │         │ │  -pty)   │ │        │ │  CLI)   │ │   │
│  │  └─────────┘ └──────────┘ └────────┘ └─────────┘ │   │
│  │                                                    │   │
│  │  ┌─────────────────────────────────────────────┐  │   │
│  │  │  Project Manager (local + remote sessions)   │  │   │
│  │  └─────────────────────────────────────────────┘  │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

### Key Technology Choices

| Concern                | Technology                     | Why                                                    |
|------------------------|--------------------------------|--------------------------------------------------------|
| Desktop framework      | Tauri v2                       | Smallest footprint, Rust backend, best perf            |
| Frontend framework     | React 19 + TypeScript          | Mature ecosystem, component model fits IDE UI          |
| Build tool              | Vite                           | Fast HMR, Tauri-compatible                             |
| Styling                | Tailwind CSS 4                 | Rapid UI, dark theme support out of box                |
| Code editor            | Monaco Editor (@monaco-editor/react) | Same engine as VS Code, multi-model/tab support |
| Terminal frontend      | xterm.js (@xterm/xterm)        | Industry standard, used by VS Code                     |
| Terminal backend (PTY) | portable-pty (Rust crate)      | Cross-platform PTY, native to Tauri's Rust core        |
| SSH client             | russh (Rust crate)             | Pure-Rust async SSH, SFTP support, no native deps      |
| Local file system      | Tauri fs plugin + std::fs      | Sandboxed local FS access                              |
| Remote file system     | SFTP via russh-sftp            | File read/write/list over SSH                          |
| Git operations         | git2 crate (libgit2) + git CLI | Worktree management, branch listing, status            |
| State management       | Zustand                        | Lightweight, no boilerplate                            |
| Icons                  | Lucide React                   | Clean, consistent icon set                             |
| Layout                 | react-resizable-panels         | Draggable split views for sidebars/main                |

### Data Flow

```
User Action (UI)
    │
    ▼
React Component
    │
    ▼ invoke('command_name', { args })
Tauri IPC Bridge
    │
    ▼
Rust Command Handler
    │
    ├──► std::fs / Tauri fs plugin   (local files)
    ├──► portable-pty                 (local terminal)
    ├──► russh + SFTP                 (remote files & terminal)
    ├──► git2 / git CLI               (git operations)
    │
    ▼ emit('event_name', { data })
Tauri Event System
    │
    ▼
React Component (updates UI)
```

### Project Model

```
Project
├── id: string (uuid)
├── name: string
├── type: "local" | "ssh"
├── connection:
│   ├── local: { path: string }
│   └── ssh: { host, port, username, authMethod, keyPath/password }
├── worktrees: Worktree[]
└── activeWorktreeId: string | null

Worktree
├── id: string
├── branch: string
├── path: string          (local path or remote path)
├── isMain: boolean
├── status: "clean" | "dirty" | "unknown"
└── ahead/behind: number

TerminalSession
├── id: string
├── worktreeId: string
├── type: "local" | "ssh"
├── ptyId: string         (backend PTY handle)
├── cwd: string

EditorTab
├── id: string
├── filePath: string
├── worktreeId: string
├── isDirty: boolean
├── language: string
```

### UI Layout

```
┌──────────────────────────────────────────────────────────────┐
│ Title Bar / Menu Bar                                           │
├────────┬────────────────────────────────────┬────────────────┤
│ Left   │ Main Area                          │ Right Sidebar   │
│ Sidebar│                                    │ (File Tree)     │
│        │ ┌────────────────────────────────┐ │                 │
│ Projects│ │ Terminal Tabs                  │ │ ▸ project/     │
│        │ │ [term1] [term2] [+]            │ │   ▸ src/       │
│ ▸ proj1│ │ ┌────────────────────────────┐ │ │     ▸ comp/    │
│   ▸ wt1│ │ │ $ git status               │ │ │       file.ts  │
│   ▸ wt2│ │ │ ...                        │ │ │     file.test  │
│ ▸ proj2│ │ └────────────────────────────┘ │ │   ▸ tests/     │
│        │ ├────────────────────────────────┤ │   package.json │
│ [+ New]│ │ Editor Tabs                    │ │   tsconfig.json│
│        │ │ [file1.ts] [file2.ts] [+]      │ │                 │
│        │ │ ┌────────────────────────────┐ │ │                 │
│        │ │ │ Monaco Editor              │ │ │                 │
│        │ │ │                            │ │ │                 │
│        │ │ │  code...                   │ │ │                 │
│        │ │ │                            │ │ │                 │
│        │ │ └────────────────────────────┘ │ │                 │
│        │ └────────────────────────────────┘ │                 │
├────────┴────────────────────────────────────┴────────────────┤
│ Status Bar                                                     │
└──────────────────────────────────────────────────────────────┘
```

The main area has two stacked zones:
1. **Terminal zone** (top) - multiple xterm.js terminal tabs
2. **Editor zone** (bottom) - multiple Monaco editor tabs

Both zones are resizable via drag handles. The terminal zone can be collapsed to give the editor full height, and vice versa.

---

## Task Breakdown

### Phase 1: Project Scaffolding & Core Setup

#### Task 1.1: Initialize Tauri v2 + React project
- Run `npm create tauri-app@latest` with React + TypeScript + Vite template
- Configure project metadata (name, bundle ID, icons)
- Set up `src/` (frontend) and `src-tauri/` (Rust backend) directory structure
- Verify `npm run tauri dev` works

#### Task 1.2: Install and configure dependencies
- Install frontend deps: `@monaco-editor/react`, `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-web-links`, `zustand`, `lucide-react`, `react-resizable-panels`, `tailwindcss`
- Install Rust deps in `Cargo.toml`: `portable-pty`, `russh`, `russh-sftp`, `git2` (or `gix`), `serde`, `uuid`, `tokio`
- Configure Tailwind CSS with dark theme defaults
- Set up base TypeScript types (Project, Worktree, TerminalSession, EditorTab)

#### Task 1.3: Set up base layout shell
- Create 3-panel layout: left sidebar, main area, right sidebar
- Use `react-resizable-panels` for draggable dividers
- Add title bar and status bar
- Apply dark theme styling
- Verify responsive resize works

### Phase 2: Project Management

#### Task 2.1: Project store & types
- Create Zustand store for projects (list, active project, CRUD)
- Define TypeScript interfaces: Project, LocalProject, SSHProject
- Implement project persistence (save/load to local config file via Tauri fs)

#### Task 2.2: Add project UI (local)
- "Add Project" button in left sidebar
- Folder picker dialog (Tauri dialog plugin) to select local directory
- Validate that selected path contains a git repo (or init one)
- Show project in sidebar with expand/collapse

#### Task 2.3: Add project UI (SSH)
- SSH connection form (host, port, username, auth method: key/password)
- "Test Connection" button that pings the SSH server
- On success, browse remote directories to select project root
- Store SSH credentials securely (Tauri keyring / OS keychain)

#### Task 2.4: Rust backend - SSH client module
- Implement SSH connection manager using `russh`
- Support password and private-key authentication
- Maintain connection pool per project
- Expose Tauri commands: `ssh_connect`, `ssh_test_connection`, `ssh_disconnect`
- Handle reconnection logic

### Phase 3: Git Worktree Management

#### Task 3.1: Rust backend - Git operations
- Use `git2` crate or shell out to `git` CLI
- Implement commands: `git_worktree_list`, `git_worktree_add`, `git_worktree_remove`
- Detect main worktree vs linked worktrees
- Get branch name and status for each worktree
- For SSH projects: run git commands over SSH

#### Task 3.2: Worktree tree UI in left sidebar
- Under each project, show expandable worktree list
- Each worktree shows: branch name, dirty/clean status indicator, ahead/behind counts
- Active worktree highlighted
- Click to switch active worktree (updates file tree + terminal cwd)

#### Task 3.3: Worktree actions
- "Add Worktree" dialog: choose branch (new or existing), path
- "Remove Worktree" with confirmation (especially if dirty)
- "Refresh" button to re-fetch worktree list
- Context menu: open in terminal, copy path

### Phase 4: File System & File Tree

#### Task 4.1: Rust backend - File system abstraction
- Create `FileSystemProvider` trait with methods: `read_dir`, `read_file`, `write_file`, `stat`, `mkdir`, `rm`, `mv`
- Implement `LocalFileSystem` using `std::fs`
- Implement `SftpFileSystem` using `russh-sftp`
- Expose Tauri commands that dispatch to the right provider based on project type

#### Task 4.2: File tree component (right sidebar)
- Recursive tree view with expand/collapse
- Lazy-load directories (don't load entire tree at once)
- Show file icons (use Lucide icons or file-extension-based mapping)
- Highlight currently open files
- Context menu: new file, new folder, rename, delete, copy path

#### Task 4.3: File tree actions
- "New File" - prompt for name, create in selected directory
- "New Folder" - prompt for name, create in selected directory
- "Rename" - inline rename in tree
- "Delete" - with confirmation
- "Refresh" - reload current directory
- File search/filter input at top of tree

### Phase 5: Code Editor (Monaco)

#### Task 5.1: Monaco editor integration
- Set up `@monaco-editor/react` with dark theme
- Configure editor options: font, tab size, word wrap, minimap
- Multi-model support (one model per open file path)

#### Task 5.2: Editor tab bar
- Tab bar above editor with open file tabs
- Show file name + dirty indicator
- Close tab button (x) per tab
- Tab switching, reorder (optional)
- Middle-click to close

#### Task 5.3: File open/save flow
- Double-click file in tree → open in editor tab
- Read file content via Tauri IPC (local or SFTP)
- Write file on save (Ctrl+S)
- Track dirty state (modified since last save)
- Prompt to save on tab close if dirty

#### Task 5.4: Editor features
- Language detection from file extension
- Basic syntax highlighting (Monaco built-in)
- Search & replace within file (Ctrl+F / Ctrl+H)
- Command palette (optional, later)

### Phase 6: Integrated Terminal

#### Task 6.1: Rust backend - PTY manager
- Use `portable-pty` to spawn local shell processes
- Maintain map of PTY sessions (id → PtyPair)
- Expose Tauri commands: `pty_spawn`, `pty_write`, `pty_resize`, `pty_kill`
- Stream PTY output to frontend via Tauri events (`pty_output`)
- Accept input from frontend via `pty_write` command

#### Task 6.2: Terminal tab UI
- Terminal tab bar in main area (top zone)
- "New Terminal" button (+)
- Each tab manages its own xterm.js instance
- Tab labels: terminal name or cwd basename

#### Task 6.3: xterm.js integration
- Initialize `Terminal` instance per tab with FitAddon
- Bind xterm input → `pty_write` IPC command
- Bind Tauri `pty_output` event → `term.write(data)`
- Handle terminal resize: FitAddon → `pty_resize(cols, rows)`
- CRITICAL: Keep terminal mounted (CSS visibility toggle), never unmount on tab switch — unmounting destroys xterm.js state

#### Task 6.4: SSH terminal support
- For SSH projects, spawn shell via `russh` channel instead of local PTY
- Same IPC interface (spawn/write/resize/kill) but backed by SSH shell
- Stream SSH shell output via events, same as local PTY
- Set initial cwd to active worktree path

#### Task 6.5: Terminal features
- Copy/paste support
- Terminal scrollback buffer
- Web links addon (clickable URLs)
- Split terminal (optional, later)
- Clear terminal command

### Phase 7: Worktree ↔ Terminal/Editor Integration

#### Task 7.1: Active worktree context
- When switching worktree:
  - Update file tree to show worktree's directory
  - Set default cwd for new terminals to worktree path
  - Optionally close editor tabs from previous worktree (or keep them)
- Terminal spawned from worktree uses worktree path as cwd

#### Task 7.2: Status bar integration
- Show active project name + worktree branch
- Show git status summary (modified/staged/untracked counts)
- Show terminal count
- Show cursor position in editor (line:col)

### Phase 8: Polish & Cross-Platform

#### Task 8.1: Keyboard shortcuts
- Ctrl+S: save file
- Ctrl+N: new file
- Ctrl+T: new terminal
- Ctrl+W: close current tab
- Ctrl+Shift+P: command palette (optional)
- Ctrl+P: quick file open (optional)
- Ctrl+B: toggle left sidebar
- Ctrl+J: toggle right sidebar

#### Task 8.2: Window state persistence
- Save/restore window size and position
- Save/restore panel sizes (sidebar widths, terminal/editor split)
- Save/restore open tabs and active tab
- Save/restore project list and last active project

#### Task 8.3: Cross-platform testing
- Test on Linux (WebKitGTK)
- Test on macOS (WKWebView)
- Test on Windows (WebView2)
- Handle platform-specific paths and shell defaults (bash/zsh/powershell)

#### Task 8.4: Error handling & UX
- Connection error toasts (SSH failures, file not found, etc.)
- Loading states for async operations
- Empty states (no projects, no worktrees, no open files)
- Confirmation dialogs for destructive actions

### Phase 9: Advanced Features (Post-MVP)

- Git diff view (inline in editor)
- Git blame annotations
- Search across files (project-wide)
- Integrated git commit/stage UI
- Split editor views
- Multiple windows
- Settings/preferences panel
- Plugin/extension system
- AI assistant integration
- Docker container terminals
- Remote port forwarding

---

## Recommended Implementation Order

```
Phase 1 (Scaffolding)     ████████████  Week 1
Phase 2 (Projects)        ████████████  Week 2
Phase 3 (Worktrees)       ████████████  Week 3
Phase 4 (File Tree)       ████████████  Week 4
Phase 5 (Editor)          ████████████  Week 5
Phase 6 (Terminal)        ████████████  Week 6-7
Phase 7 (Integration)     ████████████  Week 8
Phase 8 (Polish)          ████████████  Week 9
```

The critical path is: **Scaffolding → Projects → File Tree → Editor → Terminal → Worktree Integration**.

Worktree management (Phase 3) can be developed in parallel with file tree/editor since it primarily needs the git backend and sidebar UI, which are independent of the editor and terminal.

---

## Key Architectural Decisions

### Why Rust SSH (russh) instead of Node sidecar (ssh2)?

Using `russh` (pure-Rust SSH) in the Tauri backend:
- No need to bundle Node.js as a sidecar (saves ~40MB)
- Native async I/O with tokio
- No native dependency issues across platforms
- Single binary distribution

If russh proves problematic for certain SSH features, we can fall back to shelling out to the system `ssh` client or bundling a Node sidecar, but russh should be the first choice.

### Why portable-pty instead of node-pty?

`portable-pty` is a Rust crate from the WezTerm project:
- Native to Tauri's Rust backend (no Node.js required)
- Cross-platform (Windows ConPTY, Unix openpty)
- Battle-tested in WezTerm terminal emulator
- No native compilation issues for end users

Alternative: `tauri-plugin-pty` by Tnze wraps portable-pty specifically for Tauri v2.

### Why Monaco instead of CodeMirror?

Monaco is the same editor engine as VS Code:
- Users get familiar VS Code-like editing experience
- Built-in IntelliSense for many languages
- Multi-model support for tabbed editing (each file = one model)
- Massive ecosystem and community support
- Tradeoff: larger bundle size (~5MB), but acceptable for a desktop app

### Why Zustand instead of Redux/Context?

- Minimal boilerplate
- No provider wrapper needed
- Works naturally with React 19
- Sufficient for IDE state (projects, worktrees, tabs, terminals)
- Can slice stores by domain (projectStore, editorStore, terminalStore)

### Terminal lifecycle: CSS visibility, not conditional render

This is a critical pattern discovered by multiple teams building browser/webview terminals:

> When the terminal component unmounts, xterm.js disposes of its internal state. Remounting creates a new instance that cannot reconnect to the existing PTY session.

**Solution**: Keep all terminal components mounted at all times. Toggle visibility with CSS (`display: none` / `display: block`). This preserves the terminal instance, the PTY connection, and the scroll buffer across tab switches.
