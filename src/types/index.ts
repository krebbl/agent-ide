export type ProjectType = "local" | "ssh";

export interface LocalConnection {
  path: string;
}

export interface SSHConnection {
  host: string;
  port: number;
  username: string;
  authMethod: "key" | "password";
  keyPath?: string;
  password?: string;
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
}

export interface EditorTab {
  id: string;
  filePath: string;
  worktreeId: string;
  isDirty: boolean;
  language: string;
}
