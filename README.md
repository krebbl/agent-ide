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
| `npm run build:web` | Build browser frontend bundle (sets `VITE_TAURI=false`) |
| `npm run build:server` | Build the headless server binary in release mode |
| `npm run build:web:server` | Build web bundle and server binary |
| `npm run server` | Run the release server binary (requires `dist/`) |
| `npm run server:debug` | Run the debug server binary (requires `dist/`) |
| `npm run tauri dev` | Run full app in dev mode (frontend + Rust backend) |
| `npm run tauri:dev:worktree` | Run a dev instance isolated for the current git worktree |
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

## Multi-Worktree Development

The repo supports running separate Agent IDE dev instances from multiple git worktrees simultaneously. Each instance gets its own app identifier, Vite dev ports, and config directory so they do not collide.

```bash
# From any git worktree
npm run tauri:dev:worktree
```

The script derives a stable identifier and ports from the worktree path and branch, and sets `AGENT_IDE_CONFIG_DIR` so PTY daemon state is isolated.

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full architecture document, technology decisions, and task breakdown.

## License

MIT

## Server deployment

You can also run Agent IDE as a headless server and access it from a browser. The server serves the React frontend and exposes the same backend commands over HTTP/WebSocket.

### Build the server binary

Using npm:

```bash
export AGENT_IDE_AUTH_TOKEN=$(openssl rand -hex 32)
npm run build:web:server
```

Or use the shell script:

```bash
./scripts/build-server.sh
```

Both produce `src-tauri/target/release/agent-ide-server` and the static bundle in `dist/`. Use `npm run build:server:debug` and `npm run server:debug` for a debug build.

### Run locally

```bash
export AGENT_IDE_AUTH_TOKEN=$(openssl rand -hex 32)
npm run server
```

Or manually:

```bash
export AGENT_IDE_AUTH_TOKEN=$(openssl rand -hex 32)
export AGENT_IDE_CONFIG_DIR="$HOME/.config/agent-ide"
export AGENT_IDE_SECRET_STORE=file
export AGENT_IDE_STATIC_DIR="$(pwd)/dist"
export AGENT_IDE_PORT=3000

src-tauri/target/release/agent-ide-server
```

Open `http://localhost:3000`, paste the token, and start using Agent IDE from the browser.

### Systemd service

1. Build the binary and copy it somewhere permanent, e.g. `/opt/agent-ide`.
2. Copy the static bundle: `cp -r dist /opt/agent-ide/dist`.
3. Copy `scripts/agent-ide-server.service` to `/etc/systemd/system/agent-ide-server.service`.
4. Create `/etc/default/agent-ide-server` with at least `AGENT_IDE_AUTH_TOKEN` and any other environment variables:

```bash
AGENT_IDE_AUTH_TOKEN=your-token-here
AGENT_IDE_CONFIG_DIR=/var/lib/agent-ide
AGENT_IDE_SECRET_STORE=file
AGENT_IDE_STATIC_DIR=/opt/agent-ide/dist
AGENT_IDE_PORT=3000
SSH_AUTH_SOCK=/run/user/1000/keyring/ssh
```

5. Enable and start the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now agent-ide-server
```

Access the UI at `http://<server>:3000` and enter the token.

### SSH agent forwarding

When running on a remote server, forward your local SSH agent so Agent IDE can use your keys for SSH projects:

```bash
ssh -A user@server
```

Then set `SSH_AUTH_SOCK` to the forwarded socket path in `/etc/default/agent-ide-server` (usually `/run/user/$UID/keyring/ssh` or `/tmp/ssh-XXXXXXXX/agent.XXXX`).

### Security notes

- The bearer token is required for command and WebSocket endpoints. Serve behind a TLS-terminating reverse proxy for remote access.
- Bind `AGENT_IDE_PORT` to localhost or an internal interface unless you have another layer of authentication.
- Single-user: the running server holds one shared `AppState`.
