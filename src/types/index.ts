export type ProjectType = "local" | "ssh";

export interface LocalConnection {
  type: "local";
  path: string;
}

export interface SSHConnection {
  type: "ssh";
  host: string;
  port: number;
  username: string;
  authMethod: "key" | "password" | "agent";
  keyPath?: string;
  path?: string;
}

export interface Project {
  id: string;
  name: string;
  type: ProjectType;
  connection: LocalConnection | SSHConnection;
  worktrees: Worktree[];
  activeWorktreeId: string | null;
}

export interface Worktree {
  id: string;
  branch: string;
  path: string;
  isMain: boolean;
  status: "clean" | "dirty" | "unknown";
  ahead: number;
  behind: number;
}

export interface TerminalSession {
  id: string;
  worktreeId: string;
  type: "local" | "ssh";
  ptyId: string;
  cwd: string;
  isBusy?: boolean;
  needsInput?: boolean;
}

export interface EditorTab {
  id: string;
  filePath: string;
  worktreeId: string;
  isDirty: boolean;
  language: string;
}
