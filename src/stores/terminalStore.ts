import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "./projectStore";
import { Project } from "../types";

export interface TerminalSession {
  id: string;
  ptyId: string;
  cwd: string;
  title: string;
  type: "local" | "ssh";
  worktreeId?: string;
}

interface TerminalStore {
  sessions: TerminalSession[];
  activeSessionId: string | null;
  isCollapsed: boolean;
  addSession: (cwd?: string, type?: "local" | "ssh", worktreeId?: string) => Promise<void>;
  removeSession: (id: string) => Promise<void>;
  setActiveSession: (id: string | null) => void;
  updateSessionCwd: (id: string, cwd: string) => void;
  updateSessionTitle: (id: string, title: string) => void;
  setCollapsed: (collapsed: boolean) => void;
}

function basename(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  return segments.pop() || path || "~";
}

function resolveCwd(
  project: Project | undefined,
  cwd?: string,
  worktreeId?: string,
): { cwd: string | null; worktreeId?: string } {
  if (cwd) {
    return { cwd, worktreeId };
  }
  const store = useProjectStore.getState();
  const targetWorktreeId = worktreeId ?? store.selectedWorktreeId ?? undefined;
  const wt = project?.worktrees.find((w) => w.id === targetWorktreeId);
  if (wt) {
    return { cwd: wt.path, worktreeId: wt.id };
  }
  const connectionPath = (project?.connection as { path?: string } | undefined)?.path;
  if (connectionPath) {
    return { cwd: connectionPath, worktreeId: targetWorktreeId };
  }
  return { cwd: null, worktreeId: targetWorktreeId };
}

export const useTerminalStore = create<TerminalStore>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  isCollapsed: false,

  addSession: async (cwd, type, worktreeId) => {
    const project = useProjectStore.getState().getActiveProject();
    const resolvedType = type ?? (project?.type === "ssh" ? "ssh" : "local");
    const { cwd: resolvedCwd, worktreeId: resolvedWorktreeId } = resolveCwd(
      project,
      cwd,
      worktreeId,
    );

    const ptyId = await invoke<string>("pty_spawn", {
      cwd: resolvedCwd,
      cols: 80,
      rows: 24,
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
          worktreeId: resolvedWorktreeId,
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

  setCollapsed: (collapsed) => set({ isCollapsed: collapsed }),
}));
