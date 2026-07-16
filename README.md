# Agent IDE

A lightweight, cross-platform IDE for coding projects with git worktree management, integrated terminals, SSH support, and Monaco editor.

Built with **Tauri v2** (Rust backend) + **React 19** (TypeScript frontend).

## Prerequisites

### All platforms

- [Node.js](https://nodejs.org/) (latest LTS, v20+)
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- A C/C++ compiler (GCC/Clang on Linux/macOS, MSVC on Windows)

### Linux

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libssl-dev pkg-config
```

### macOS

```bash
# Xcode command line tools (provides Clang and required libraries)
xcode-select --install
```

### Windows

```bash
# Install Microsoft C++ Build Tools via Visual Studio Installer
# https://visualstudio.microsoft.com/visual-cpp-build-tools/
# Select "Desktop development with C++" workload

# Install WebView2 (usually pre-installed on Windows 10/11)
```

## Getting Started

```bash
# Clone the repository
git clone https://github.com/krebbl/agent-ide.git
cd agent-ide

# Install frontend dependencies
npm install

# Run in development mode (launches Tauri window with hot reload)
npm run tauri dev
```

The app window (1280x800) will open with the 3-panel IDE layout.

## Available Scripts

| Command | Description |
|---------|-------------|
| `npm run dev` | Start Vite dev server (frontend only, no Tauri window) |
| `npm run build` | Build frontend for production (`tsc && vite build`) |
| `npm run tauri dev` | Run full app in dev mode (frontend + Rust backend) |
| `npm run tauri build` | Build production desktop app (creates installable bundle) |

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop framework | Tauri v2 |
| Frontend | React 19, TypeScript, Vite |
| Styling | Tailwind CSS v4 (Catppuccin Mocha dark theme) |
| Code editor | Monaco Editor |
| Terminal | xterm.js + portable-pty (Rust) |
| SSH | russh + russh-sftp (Rust) |
| Git operations | git2 (libgit2 bindings) |
| State management | Zustand |

## Project Structure

```
agent-ide/
├── src/                    # Frontend (React + TypeScript)
│   ├── components/
│   │   ├── layout/         # AppLayout, TitleBar, StatusBar
│   │   ├── sidebar/        # LeftSidebar (projects/worktrees), RightSidebar (file tree)
│   │   └── main/           # MainArea, TerminalZone, EditorZone
│   ├── types/              # TypeScript type definitions
│   ├── App.tsx
│   └── index.css           # Tailwind + theme variables
├── src-tauri/              # Backend (Rust)
│   ├── src/
│   │   ├── lib.rs          # Tauri app setup, plugin registration
│   │   └── main.rs         # Entry point
│   ├── Cargo.toml          # Rust dependencies
│   ├── tauri.conf.json     # Tauri configuration
│   └── capabilities/       # Permission capabilities
├── ARCHITECTURE.md         # Architecture document and task breakdown
└── package.json
```

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full architecture document, technology decisions, and task breakdown.

## License

MIT
