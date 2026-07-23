import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { DaemonSessionMeta, Pane, LeafPane, SplitPane, TerminalTab } from "../types";
import { useProjectStore } from "./projectStore";

const WORKTREE_TAB_MAP_KEY = "agent-ide:worktree-tab-map";

function worktreeKey(projectId: string, worktreeId: string): string {
  return `${projectId}:${worktreeId}`;
}

function loadWorktreeTabMap(): Record<string, string> {
  try {
    const raw = localStorage.getItem(WORKTREE_TAB_MAP_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function persistWorktreeTabMap(map: Record<string, string>) {
  try {
    localStorage.setItem(WORKTREE_TAB_MAP_KEY, JSON.stringify(map));
  } catch {
    // ignore quota errors
  }
}

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
  processRunning?: boolean;
  hasUnseenActivity?: boolean;
}

interface TerminalStore {
  sessions: TerminalSession[];
  tabs: TerminalTab[];
  activeTabId: string | null;
  activeSessionId: string | null;
  isCollapsed: boolean;
  worktreeTabMap: Record<string, string>;

  getWorktreeTabId: (projectId: string, worktreeId: string) => string | null;

  addSession: (
    cwd?: string,
    type?: "local" | "ssh",
    projectId?: string,
    worktreeId?: string,
  ) => Promise<void>;
  removeSession: (id: string) => Promise<void>;
  restoreSessions: () => Promise<void>;
  setActiveSession: (id: string | null) => void;
  updateSessionCwd: (id: string, cwd: string) => void;
  updateSessionTitle: (id: string, title: string) => void;
  updateSessionByPtyId: (ptyId: string, updates: Partial<TerminalSession>) => void;
  setCollapsed: (collapsed: boolean) => void;
  focusSession: (sessionId: string) => void;
  setSessionActivity: (
    id: string,
    activity: { isBusy: boolean; needsInput: boolean },
  ) => void;
  setProcessRunning: (id: string, running: boolean) => void;
  setSessionUnseenActivity: (sessionId: string, value: boolean) => void;
  markSessionSeen: (sessionId: string) => void;

  splitPane: (sessionId: string, direction: "horizontal" | "vertical") => Promise<void>;
  closePane: (paneId: string) => Promise<void>;
  focusPane: (paneId: string) => void;
  navigatePane: (direction: "up" | "down" | "left" | "right") => void;
  resizePane: (paneId: string, sizes: [number, number]) => void;
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

export function findLeaf(root: Pane, paneId: string): LeafPane | null {
  if (root.type === "leaf") {
    return root.id === paneId ? root : null;
  }
  return findLeaf(root.children[0], paneId) ?? findLeaf(root.children[1], paneId);
}

export function findLeafBySession(root: Pane, sessionId: string): LeafPane | null {
  if (root.type === "leaf") {
    return root.sessionId === sessionId ? root : null;
  }
  return findLeafBySession(root.children[0], sessionId) ?? findLeafBySession(root.children[1], sessionId);
}

export function collectLeaves(root: Pane): LeafPane[] {
  if (root.type === "leaf") return [root];
  return [...collectLeaves(root.children[0]), ...collectLeaves(root.children[1])];
}

function getFirstLeaf(root: Pane): LeafPane {
  if (root.type === "leaf") return root;
  return getFirstLeaf(root.children[0]);
}

function getLastLeaf(root: Pane): LeafPane {
  if (root.type === "leaf") return root;
  return getLastLeaf(root.children[1]);
}

function replacePane(root: Pane, targetId: string, replacement: Pane): Pane {
  if (root.id === targetId) return replacement;
  if (root.type === "split") {
    return {
      ...root,
      children: [
        replacePane(root.children[0], targetId, replacement) as typeof root.children[0],
        replacePane(root.children[1], targetId, replacement) as typeof root.children[1],
      ],
    };
  }
  return root;
}

function navigateFromLeaf(
  root: Pane,
  focusedId: string,
  direction: "up" | "down" | "left" | "right",
): string | null {
  const horizTarget = direction === "right" ? 1 : 0;
  const vertTarget = direction === "down" ? 1 : 0;
  function walk(
    node: Pane,
    ancestors: { split: SplitPane; index: 0 | 1 }[],
  ): string | null {
    if (node.type === "leaf" && node.id === focusedId) {
      for (let i = ancestors.length - 1; i >= 0; i--) {
        const { split, index } = ancestors[i];
        if (direction === "left" || direction === "right") {
          if (split.direction !== "horizontal") continue;
          const targetIdx = horizTarget;
          if (index === targetIdx) continue;
          const drill = targetIdx === 0 ? getFirstLeaf : getLastLeaf;
          return drill(split.children[targetIdx]).id;
        } else {
          if (split.direction !== "vertical") continue;
          const targetIdx = vertTarget;
          if (index === targetIdx) continue;
          const drill = targetIdx === 0 ? getFirstLeaf : getLastLeaf;
          return drill(split.children[targetIdx]).id;
        }
      }
      return null;
    }
    if (node.type === "split") {
      for (let ci = 0; ci < 2; ci++) {
        const result = walk(node.children[ci], [
          ...ancestors,
          { split: node, index: ci as 0 | 1 },
        ]);
        if (result !== null) return result;
      }
    }
    return null;
  }

  return walk(root, []);
}

function removePaneFromTree(root: Pane, paneId: string): Pane | null {
  if (root.type === "leaf") {
    return root.id === paneId ? null : root;
  }
  const left = removePaneFromTree(root.children[0], paneId);
  const right = removePaneFromTree(root.children[1], paneId);

  if (left === null && right === null) return null;
  if (left === null) return right!;
  if (right === null) return left!;

  if (left !== root.children[0] || right !== root.children[1]) {
    return { ...root, children: [left, right] };
  }
  return root;
}

export const useTerminalStore = create<TerminalStore>((set, get) => ({
  sessions: [],
  tabs: [],
  activeTabId: null,
  activeSessionId: null,
  isCollapsed: false,
  worktreeTabMap: loadWorktreeTabMap(),

  getWorktreeTabId: (projectId, worktreeId) => {
    return get().worktreeTabMap[worktreeKey(projectId, worktreeId)] ?? null;
  },

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

    const tab = get().tabs.find((t) => t.id === get().activeTabId);
    if (tab) {
      const leaf = findLeafBySession(tab.rootPane, sessionId);
      if (leaf) {
        const pid = session.projectId;
        const wid = session.worktreeId;
        set((state) => ({
          activeTabId: tab.id,
          activeSessionId: sessionId,
          worktreeTabMap:
            pid && wid
              ? { ...state.worktreeTabMap, [worktreeKey(pid, wid)]: tab.id }
              : state.worktreeTabMap,
          tabs: state.tabs.map((t) =>
            t.id === tab.id ? { ...t, focusedPaneId: leaf.id } : t,
          ),
        }));
        return;
      }
    }

    const matchingTab = get().tabs.find((t) => {
      return findLeafBySession(t.rootPane, sessionId) !== null;
    });
    if (matchingTab) {
      const matchingLeaf = findLeafBySession(matchingTab.rootPane, sessionId);
      const pid = session.projectId;
      const wid = session.worktreeId;
      set((state) => ({
        activeTabId: matchingTab.id,
        activeSessionId: sessionId,
        worktreeTabMap:
          pid && wid
            ? { ...state.worktreeTabMap, [worktreeKey(pid, wid)]: matchingTab.id }
            : state.worktreeTabMap,
        tabs: state.tabs.map((t) =>
          t.id === matchingTab.id && matchingLeaf
            ? { ...t, focusedPaneId: matchingLeaf.id }
            : t,
        ),
      }));
    }
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
      worktreeId: resolvedWorktreeId,
      sessionType: resolvedType,
    });

    const sessionId = crypto.randomUUID();
    const paneId = crypto.randomUUID();
    const tabId = crypto.randomUUID();
    const displayCwd = resolvedCwd ?? "~";

    const leaf: LeafPane = {
      type: "leaf",
      id: paneId,
      sessionId,
    };

    const tab: TerminalTab = {
      id: tabId,
      rootPane: leaf,
      focusedPaneId: paneId,
      projectId: resolvedProjectId,
      worktreeId: resolvedWorktreeId,
    };

    set((state) => ({
      sessions: [
        ...state.sessions,
        {
          id: sessionId,
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
      tabs: [...state.tabs, tab],
      activeTabId: tabId,
      activeSessionId: sessionId,
      worktreeTabMap:
        resolvedProjectId && resolvedWorktreeId
          ? {
              ...state.worktreeTabMap,
              [worktreeKey(resolvedProjectId, resolvedWorktreeId)]: tabId,
            }
          : state.worktreeTabMap,
    }));
  },

  removeSession: async (id) => {
    const session = get().sessions.find((s) => s.id === id);
    if (!session) return;

    const tab = get().tabs.find((t) =>
      findLeafBySession(t.rootPane, id) !== null,
    );
    if (!tab) return;

    const leaf = findLeafBySession(tab.rootPane, id);
    if (!leaf) return;

    const newRoot = removePaneFromTree(tab.rootPane, leaf.id);

    set((state) => {
      let tabs = state.tabs;
      let activeTabId = state.activeTabId;
      let activeSessionId = state.activeSessionId;

      if (newRoot === null) {
        tabs = state.tabs.filter((t) => t.id !== tab.id);
        if (activeTabId === tab.id) {
          const sameWorktreeTabs = tabs.filter(
            (t) =>
              t.projectId === tab.projectId &&
              t.worktreeId === tab.worktreeId,
          );
          activeTabId =
            sameWorktreeTabs.length > 0
              ? sameWorktreeTabs[sameWorktreeTabs.length - 1].id
              : null;
          activeSessionId = null;
          if (activeTabId) {
            const newActive = tabs.find((t) => t.id === activeTabId);
            if (newActive) {
              const focused = findLeaf(newActive.rootPane, newActive.focusedPaneId);
              activeSessionId = focused?.sessionId ?? null;
            }
          }
        }
      } else {
        const newFocusedPaneId = findLeaf(newRoot, tab.focusedPaneId)
          ? tab.focusedPaneId
          : getFirstLeaf(newRoot).id;
        tabs = state.tabs.map((t) =>
          t.id === tab.id
            ? { ...t, rootPane: newRoot, focusedPaneId: newFocusedPaneId }
            : t,
        );
        if (activeSessionId === id && activeTabId === tab.id) {
          const newFocused = findLeaf(newRoot, newFocusedPaneId);
          activeSessionId = newFocused?.sessionId ?? null;
        }
      }

      return {
        sessions: state.sessions.filter((s) => s.id !== id),
        tabs,
        activeTabId,
        activeSessionId,
        worktreeTabMap: (() => {
          if (!tab.projectId || !tab.worktreeId) return state.worktreeTabMap;
          const key = worktreeKey(tab.projectId, tab.worktreeId);
          const savedTabId = state.worktreeTabMap[key];
          if (savedTabId === tab.id || savedTabId === undefined) {
            const remaining = tabs.filter(
              (t) =>
                t.projectId === tab.projectId &&
                t.worktreeId === tab.worktreeId,
            );
            if (remaining.length > 0 && activeTabId) {
              return { ...state.worktreeTabMap, [key]: activeTabId };
            }
            if (remaining.length === 0) {
              const { [key]: _, ...rest } = state.worktreeTabMap;
              return rest;
            }
          }
          return state.worktreeTabMap;
        })(),
      };
    });

    await invoke("pty_kill", { sessionId: session.ptyId }).catch(() => {});
  },

  restoreSessions: async () => {
    const sessions = await invoke<DaemonSessionMeta[]>("pty_list_sessions").catch(() => []);
    if (sessions.length === 0) return;

    set((state) => {
      const existingPtyIds = new Set(state.sessions.map((s) => s.ptyId));
      const toAdd: TerminalSession[] = sessions
        .filter((meta) => !existingPtyIds.has(meta.sessionId))
        .map((meta) => ({
          id: crypto.randomUUID(),
          ptyId: meta.sessionId,
          cwd: meta.cwd ?? "~",
          title: meta.title,
          type: meta.sessionType as "local" | "ssh",
          projectId: meta.projectId,
          worktreeId: meta.worktreeId,
          isBusy: meta.isBusy,
          needsInput: !meta.isBusy,
        }));

      if (toAdd.length === 0) return state;

      const newTabs: TerminalTab[] = toAdd.map((s) => {
        const paneId = crypto.randomUUID();
        const leaf: LeafPane = { type: "leaf", id: paneId, sessionId: s.id };
        return {
          id: crypto.randomUUID(),
          rootPane: leaf,
          focusedPaneId: paneId,
          projectId: s.projectId,
          worktreeId: s.worktreeId,
        };
      });

      return {
        sessions: [...state.sessions, ...toAdd],
        tabs: [...state.tabs, ...newTabs],
        activeTabId: state.activeTabId ?? newTabs[newTabs.length - 1]?.id ?? null,
        activeSessionId:
          state.activeSessionId ?? toAdd[toAdd.length - 1]?.id ?? null,
      };
    });
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

  updateSessionByPtyId: (ptyId, updates) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.ptyId === ptyId ? { ...s, ...updates } : s,
      ),
    })),

  setSessionActivity: (id, activity) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, ...activity } : s,
      ),
    })),

  setProcessRunning: (id, running) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, processRunning: running } : s,
      ),
    })),

  setSessionUnseenActivity: (sessionId, value) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === sessionId ? { ...s, hasUnseenActivity: value } : s,
      ),
    })),

  markSessionSeen: (sessionId) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === sessionId && s.hasUnseenActivity
          ? { ...s, hasUnseenActivity: false }
          : s,
      ),
    })),

  splitPane: async (sessionId, direction) => {
    const state = get();
    const tab = state.tabs.find((t) => t.id === state.activeTabId);
    if (!tab) return;

    const existingLeaf = findLeafBySession(tab.rootPane, sessionId);
    if (!existingLeaf) return;

    const session = state.sessions.find((s) => s.id === sessionId);
    if (!session) return;

    const ptyId = await invoke<string>("pty_spawn", {
      cwd: session.cwd,
      cols: 80,
      rows: 24,
      projectId: session.projectId,
      worktreeId: session.worktreeId,
      sessionType: session.type,
    });

    const newSessionId = crypto.randomUUID();
    const newPaneId = crypto.randomUUID();
    const newLeaf: LeafPane = {
      type: "leaf",
      id: newPaneId,
      sessionId: newSessionId,
    };

    const splitId = crypto.randomUUID();
    const split: SplitPane = {
      type: "split",
      id: splitId,
      direction,
      children: [existingLeaf, newLeaf],
      sizes: [50, 50],
    };

    const newRoot = replacePane(tab.rootPane, existingLeaf.id, split);

    set((state) => ({
      sessions: [
        ...state.sessions,
        {
          id: newSessionId,
          ptyId,
          cwd: session.cwd,
          title: basename(session.cwd),
          type: session.type,
          projectId: session.projectId,
          worktreeId: session.worktreeId,
          isBusy: false,
          needsInput: true,
        },
      ],
      tabs: state.tabs.map((t) =>
        t.id === tab.id
          ? { ...t, rootPane: newRoot, focusedPaneId: newPaneId }
          : t,
      ),
      activeSessionId: newSessionId,
    }));
  },

  closePane: async (paneId) => {
    const state = get();
    const tab = state.tabs.find((t) => t.id === state.activeTabId);
    if (!tab) return;

    const leaf = findLeaf(tab.rootPane, paneId);
    if (!leaf) return;

    await get().removeSession(leaf.sessionId);
  },

  focusPane: (paneId) => {
    set((state) => {
      const tab = state.tabs.find((t) => t.id === state.activeTabId);
      if (!tab) return state;

      const leaf = findLeaf(tab.rootPane, paneId);
      if (!leaf) return state;

      return {
        tabs: state.tabs.map((t) =>
          t.id === tab.id ? { ...t, focusedPaneId: paneId } : t,
        ),
        activeSessionId: leaf.sessionId,
      };
    });
  },

  navigatePane: (direction) => {
    set((state) => {
      const tab = state.tabs.find((t) => t.id === state.activeTabId);
      if (!tab) return state;

      const nextPaneId = navigateFromLeaf(
        tab.rootPane,
        tab.focusedPaneId,
        direction,
      );
      if (!nextPaneId) return state;

      const leaf = findLeaf(tab.rootPane, nextPaneId);
      if (!leaf) return state;

      return {
        tabs: state.tabs.map((t) =>
          t.id === tab.id ? { ...t, focusedPaneId: nextPaneId } : t,
        ),
        activeSessionId: leaf.sessionId,
      };
    });
  },

  resizePane: (paneId, sizes) => {
    set((state) => {
      const tab = state.tabs.find((t) => t.id === state.activeTabId);
      if (!tab) return state;

      function updateSizes(root: Pane): Pane {
        if (root.id === paneId && root.type === "split") {
          return { ...root, sizes };
        }
        if (root.type === "split") {
          const newLeft = updateSizes(root.children[0]);
          const newRight = updateSizes(root.children[1]);
          if (newLeft !== root.children[0] || newRight !== root.children[1]) {
            return { ...root, children: [newLeft, newRight] };
          }
        }
        return root;
      }

      return {
        tabs: state.tabs.map((t) =>
          t.id === tab.id ? { ...t, rootPane: updateSizes(t.rootPane) } : t,
        ),
      };
    });
  },
}));

useTerminalStore.subscribe((state, prevState) => {
  if (state.worktreeTabMap !== prevState.worktreeTabMap) {
    persistWorktreeTabMap(state.worktreeTabMap);
  }

  const updates: Array<{ id: string; hasUnseenActivity: true }> = [];
  for (const session of state.sessions) {
    const prev = prevState.sessions.find((s) => s.id === session.id);
    if (!prev) continue;
    const wasBusy = prev.isBusy === true || prev.processRunning === true;
    const isNowIdle = session.isBusy !== true && session.processRunning !== true;
    if (wasBusy && isNowIdle && session.id !== state.activeSessionId && !session.hasUnseenActivity) {
      updates.push({ id: session.id, hasUnseenActivity: true });
    }
  }

  if (updates.length > 0) {
    useTerminalStore.setState((s) => ({
      sessions: s.sessions.map((session) => {
        const update = updates.find((u) => u.id === session.id);
        return update ? { ...session, ...update } : session;
      }),
    }));
  }
});
