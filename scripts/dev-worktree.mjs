#!/usr/bin/env node
import { execSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import os from "node:os";
import path from "node:path";

function git(args) {
  return execSync(`git ${args}`, { encoding: "utf8", stdio: ["pipe", "pipe", "ignore"] }).trim();
}

function sanitizeIdentifierSegment(name) {
  return (
    name
      .toLowerCase()
      .replace(/[^a-z0-9.-]/g, "-")
      .replace(/-+/g, "-")
      .replace(/^[-.]+|[-.]+$/g, "") || "worktree"
  );
}

function computePorts(worktreePath, isMainWorktree) {
  if (isMainWorktree) {
    return { port: 1420, hmrPort: 1421 };
  }
  const hash = createHash("sha256").update(worktreePath).digest("hex");
  const offset = parseInt(hash.slice(0, 8), 16) % 1000;
  const port = 1420 + offset * 2;
  return { port, hmrPort: port + 1 };
}

function configDir(suffix) {
  const home = os.homedir();
  let base;
  switch (process.platform) {
    case "darwin":
      base = path.join(home, "Library", "Application Support", "agent-ide");
      break;
    case "win32":
      base = path.join(process.env.LOCALAPPDATA || path.join(home, "AppData", "Local"), "agent-ide");
      break;
    default:
      base = path.join(process.env.XDG_CONFIG_HOME || path.join(home, ".config"), "agent-ide");
      break;
  }
  return suffix ? path.join(base, suffix) : base;
}

function main() {
  const branch = git("rev-parse --abbrev-ref HEAD");
  const worktreePath = git("rev-parse --show-toplevel");
  const gitDir = git("rev-parse --git-dir");
  const isMainWorktree = gitDir === ".git" || gitDir.endsWith("/.git");
  const suffix = isMainWorktree ? "" : sanitizeIdentifierSegment(branch);
  const { port, hmrPort } = computePorts(worktreePath, isMainWorktree);
  const identifier = isMainWorktree
    ? "com.krebbl.agent-ide"
    : `com.krebbl.agent-ide.${suffix}`;

  process.env.AGENT_IDE_PORT = String(port);
  process.env.AGENT_IDE_HMR_PORT = String(hmrPort);
  process.env.AGENT_IDE_CONFIG_DIR = configDir(suffix);

  const tauriConfig = JSON.stringify({
    identifier,
    build: {
      devUrl: `http://localhost:${port}`,
    },
  });

  console.log(`Agent IDE worktree dev`);
  console.log(`  worktree : ${worktreePath}`);
  console.log(`  branch   : ${branch}`);
  console.log(`  main     : ${isMainWorktree}`);
  console.log(`  identifier: ${identifier}`);
  console.log(`  devUrl   : http://localhost:${port}`);
  console.log(`  hmrPort  : ${hmrPort}`);
  console.log(`  configDir: ${process.env.AGENT_IDE_CONFIG_DIR}`);
  console.log();

  const child = spawn("npx", ["tauri", "dev", "--config", tauriConfig], {
    stdio: "inherit",
    cwd: process.cwd(),
  });

  child.on("exit", (code) => {
    process.exit(code ?? 0);
  });
}

main();
