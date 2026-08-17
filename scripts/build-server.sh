#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$ROOT_DIR"

echo "Installing frontend dependencies..."
npm ci

echo "Building web frontend..."
VITE_TAURI=false npm run build:web

echo "Building server binary..."
cd "$ROOT_DIR/src-tauri"
cargo build --release --bin agent-ide-server

echo ""
echo "Build complete."
echo ""
echo "Run locally with:"
echo "  AGENT_IDE_AUTH_TOKEN=\$(openssl rand -hex 32) \\\\"
echo "    AGENT_IDE_CONFIG_DIR=\"\$HOME/.config/agent-ide\" \\\\"
echo "    AGENT_IDE_SECRET_STORE=file \\\\"
echo "    AGENT_IDE_STATIC_DIR=\"$ROOT_DIR/dist\" \\\\"
echo "    AGENT_IDE_PORT=3000 \\\\"
echo "    $ROOT_DIR/src-tauri/target/release/agent-ide-server"
