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
  title: string;
  isBusy?: boolean;
  needsInput?: boolean;
  processRunning?: boolean;
  hasUnseenActivity?: boolean;
}

export interface LeafPane {
  type: "leaf";
  id: string;
  sessionId: string;
}

export interface SplitPane {
  type: "split";
  id: string;
  direction: "horizontal" | "vertical";
  children: [Pane, Pane];
  sizes: [number, number];
}

export type Pane = LeafPane | SplitPane;

export interface TerminalTab {
  id: string;
  rootPane: Pane;
  focusedPaneId: string;
  projectId?: string;
  worktreeId?: string;
}

export interface DaemonSessionMeta {
  sessionId: string;
  sessionType: string;
  cwd?: string;
  title: string;
  isBusy: boolean;
  projectId?: string;
  worktreeId?: string;
  cols: number;
  rows: number;
}

export interface EditorTab {
  id: string;
  filePath: string;
  worktreeId: string;
  isDirty: boolean;
  language: string;
}

export type AgentId =
  | "claude"
  | "amp"
  | "codex"
  | "gemini"
  | "mastracode"
  | "opencode"
  | "pi"
  | "copilot"
  | "cursor-agent";

export type PromptTransport = "argv" | "stdin";

export interface AgentStatus {
  id: AgentId;
  label: string;
  description: string;
  command: string[];
  promptTransport: PromptTransport;
  enabled: boolean;
  installed: boolean;
  binaryPath: string | null;
}
