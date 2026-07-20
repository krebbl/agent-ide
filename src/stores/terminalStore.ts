import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "./projectStore";

export interface TerminalSession {
  id: string;
  ptyId: string;
  cwd: string;
  title: string;
  type: "local" | "ssh";
  projectId?: string;
  worktreeId?: string;
  isBusy?: boolean;
  needsInput?: boolean;
}

interface TerminalStore {
  sessions: TerminalSession[];
  activeSessionId: string | null;
  isCollapsed: boolean;
  addSession: (
    cwd?: string,
    type?: "local" | "ssh",
    projectId?: string,
    worktreeId?: string,
  ) => Promise<void>;
  removeSession: (id: string) => Promise<void>;
  setActiveSession: (id: string | null) => void;
  updateSessionCwd: (id: string, cwd: string) => void;
  updateSessionTitle: (id: string, title: string) => void;
  setCollapsed: (collapsed: boolean) => void;
  focusSession: (sessionId: string) => void;
  setSessionActivity: (
    id: string,
    activity: { isBusy: boolean; needsInput: boolean },
  ) => void;
}

function basename(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  return segments.pop() || path || "~";
}

function findWorktreePath(
  store: ReturnType<typeof useProjectStore.getState>,
  projectId?: string,
  worktreeId?: string,
): { path: string | null; projectId?: string; worktreeId?: string } {
  if (!projectId && !worktreeId) {
    projectId = store.activeProjectId ?? undefined;
    worktreeId = store.selectedWorktreeId ?? undefined;
  }

  const project = store.projects.find((p) => p.id === projectId);
  if (!project) {
    return { path: null, projectId, worktreeId };
  }

  const wt = worktreeId
    ? project.worktrees.find((w) => w.id === worktreeId)
    : undefined;
  if (wt) {
    return { path: wt.path, projectId, worktreeId: wt.id };
  }

  const connectionPath = (project.connection as { path?: string }).path;
  if (connectionPath) {
    return { path: connectionPath, projectId, worktreeId };
  }

  return { path: null, projectId, worktreeId };
}

function resolveCwd(
  cwd?: string,
  projectId?: string,
  worktreeId?: string,
): { cwd: string | null; projectId?: string; worktreeId?: string } {
  if (cwd) {
    return { cwd, projectId, worktreeId };
  }
  const store = useProjectStore.getState();
  const result = findWorktreePath(store, projectId, worktreeId);
  return {
    cwd: result.path,
    projectId: result.projectId,
    worktreeId: result.worktreeId,
  };
}

export const useTerminalStore = create<TerminalStore>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  isCollapsed: false,

  setCollapsed: (collapsed) => set({ isCollapsed: collapsed }),

  focusSession: (sessionId) => {
    set({ isCollapsed: false });
    const session = get().sessions.find((s) => s.id === sessionId);
    if (!session) return;

    const projectStore = useProjectStore.getState();
    if (session.projectId && session.worktreeId) {
      projectStore
        .setActiveWorktree(session.projectId, session.worktreeId)
        .catch(() => {});
    } else if (session.projectId) {
      projectStore.setActiveProject(session.projectId);
    }

    set({ activeSessionId: session.id });
  },

  addSession: async (cwd, type, projectId, worktreeId) => {
    const store = useProjectStore.getState();
    const activeProject =
      store.projects.find((p) => p.id === (projectId ?? store.activeProjectId));
    const resolvedType =
      type ?? (activeProject?.type === "ssh" ? "ssh" : "local");

    const { cwd: resolvedCwd, projectId: resolvedProjectId, worktreeId: resolvedWorktreeId } =
      resolveCwd(cwd, projectId, worktreeId);

    const ptyId = await invoke<string>("pty_spawn", {
      cwd: resolvedCwd,
      cols: 80,
      rows: 24,
      projectId: resolvedProjectId,
      sessionType: resolvedType,
    });

    const id = crypto.randomUUID();
    const displayCwd = resolvedCwd ?? "~";
    set((state) => ({
      sessions: [
        ...state.sessions,
        {
          id,
          ptyId,
          cwd: displayCwd,
          title: basename(displayCwd),
          type: resolvedType,
          projectId: resolvedProjectId,
          worktreeId: resolvedWorktreeId,
          isBusy: false,
          needsInput: true,
        },
      ],
      activeSessionId: id,
    }));
  },

  removeSession: async (id) => {
    const session = get().sessions.find((s) => s.id === id);
    set((state) => {
      const remaining = state.sessions.filter((s) => s.id !== id);
      const activeSessionId =
        state.activeSessionId === id
          ? remaining.length > 0
            ? remaining[remaining.length - 1].id
            : null
          : state.activeSessionId;
      return { sessions: remaining, activeSessionId };
    });
    if (session) {
      await invoke("pty_kill", { sessionId: session.ptyId }).catch(() => {});
    }
  },

  setActiveSession: (id) => set({ activeSessionId: id }),

  updateSessionCwd: (id, cwd) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, cwd, title: basename(cwd) } : s,
      ),
    })),

  updateSessionTitle: (id, title) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, title } : s,
      ),
    })),

  setSessionActivity: (id, activity) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, ...activity } : s,
      ),
    })),
}));
