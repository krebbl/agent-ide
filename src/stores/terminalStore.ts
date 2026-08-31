import { create } from "zustand";
import { invoke } from "../services/ipc";
import { DaemonSessionMeta, Pane, LeafPane, SplitPane, TerminalTab } from "../types";
import { useProjectStore } from "./projectStore";

const WORKTREE_TAB_MAP_KEY = "agent-ide:worktree-tab-map";
const LAYOUT_KEY = "agent-ide:terminal-layout";

// In the persisted layout, leaf sessionId holds the daemon ptyId (stable
// across restarts), not the ephemeral frontend session id.
interface PersistedLayout {
  tabs: TerminalTab[];
  activeTabId: string | null;
}

function loadPersistedLayout(): PersistedLayout | null {
  try {
    const raw = localStorage.getItem(LAYOUT_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (!parsed || !Array.isArray(parsed.tabs)) return null;
    return parsed as PersistedLayout;
  } catch {
    return null;
  }
}

function toPersistedPane(root: Pane, sessions: TerminalSession[]): Pane {
  if (root.type === "leaf") {
    const session = sessions.find((s) => s.id === root.sessionId);
    return { ...root, sessionId: session?.ptyId ?? root.sessionId };
  }
  return {
    ...root,
    children: [
      toPersistedPane(root.children[0], sessions),
      toPersistedPane(root.children[1], sessions),
    ],
  };
}

function persistLayout(state: {
  tabs: TerminalTab[];
  sessions: TerminalSession[];
  activeTabId: string | null;
}) {
  try {
    const layout: PersistedLayout = {
      tabs: state.tabs.map((t) => ({
        ...t,
        rootPane: toPersistedPane(t.rootPane, state.sessions),
      })),
      activeTabId: state.activeTabId,
    };
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
  } catch {
    // ignore quota errors
  }
}

function rebuildPane(
  root: Pane,
  sessionIdByPtyId: Map<string, string>,
): Pane | null {
  if (root.type === "leaf") {
    const sessionId = sessionIdByPtyId.get(root.sessionId);
    return sessionId ? { ...root, sessionId } : null;
  }
  const left = rebuildPane(root.children[0], sessionIdByPtyId);
  const right = rebuildPane(root.children[1], sessionIdByPtyId);
  if (left && right) return { ...root, children: [left, right] };
  return left ?? right;
}

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
  agentName?: string;
  agentActive?: boolean;
  isBusy?: boolean;
  needsInput?: boolean;
  processRunning?: boolean;
  hasUnseenActivity?: boolean;
  /** Epoch ms when this session last became busy; cleared when idle. Used to
   *  decide whether the session is long-running enough to show in Active. */
  busySince?: number;
  /** Epoch ms when this session was added. Stable ordering key for the
   *  Active list so sessions don't jump when the title/agent name changes. */
  createdAt?: number;
  /** Epoch ms when this session was last focused (tab click, Active click,
   *  agent search jump). Recency key for the agent search default selection. */
  lastActiveAt?: number;
}

/** Derive `busySince` for a session given an incoming update. Sets it on the
 *  busy transition, clears it on idle, preserves it otherwise. */
function applyBusyTiming(
  session: TerminalSession,
  updates: Partial<TerminalSession>,
): Partial<TerminalSession> {
  if (updates.isBusy === undefined) return updates;
  if (updates.isBusy) {
    return session.isBusy ? updates : { ...updates, busySince: Date.now() };
  }
  return session.busySince === undefined ? updates : { ...updates, busySince: undefined };
}

interface TerminalStore {
  sessions: TerminalSession[];
  tabs: TerminalTab[];
  activeTabId: string | null;
  activeSessionId: string | null;
  isCollapsed: boolean;
  searchOpenSessionId: string | null;
  worktreeTabMap: Record<string, string>;

  getWorktreeTabId: (projectId: string, worktreeId: string) => string | null;

  addSession: (
    cwd?: string,
    type?: "local" | "ssh",
    projectId?: string,
    worktreeId?: string,
    argv?: string[],
  ) => Promise<void>;
  removeSession: (id: string) => Promise<void>;
  restoreSessions: () => Promise<void>;
  setActiveSession: (id: string | null) => void;
  updateSessionCwd: (id: string, cwd: string) => void;
  updateSessionTitle: (id: string, title: string) => void;
  updateSessionByPtyId: (ptyId: string, updates: Partial<TerminalSession>) => void;
  setCollapsed: (collapsed: boolean) => void;
  setSearchOpen: (id: string | null) => void;
  focusSession: (sessionId: string) => void;
  setSessionActivity: (
    id: string,
    activity: { isBusy?: boolean; needsInput?: boolean },
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
  searchOpenSessionId: null,
  worktreeTabMap: loadWorktreeTabMap(),

  getWorktreeTabId: (projectId, worktreeId) => {
    return get().worktreeTabMap[worktreeKey(projectId, worktreeId)] ?? null;
  },

  setCollapsed: (collapsed) => set({ isCollapsed: collapsed }),

  setSearchOpen: (id) => set({ searchOpenSessionId: id }),
  focusSession: (sessionId) => {
    set({ isCollapsed: false });
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === sessionId
          ? { ...s, hasUnseenActivity: false, lastActiveAt: Date.now() }
          : s,
      ),
    }));
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

  addSession: async (cwd, type, projectId, worktreeId, argv) => {
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
      argv: argv ?? null,
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
          createdAt: Date.now(),
          lastActiveAt: Date.now(),
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
        searchOpenSessionId:
          state.searchOpenSessionId === id ? null : state.searchOpenSessionId,
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
          isBusy: false,
          needsInput: false,
          agentName: meta.agentName,
          agentActive: meta.agentActive,
          createdAt: meta.createdAt ?? Date.now(),
        }));

      if (toAdd.length === 0) return state;

      const allSessions = [...state.sessions, ...toAdd];
      const sessionIdByPtyId = new Map(
        allSessions.map((s) => [s.ptyId, s.id] as const),
      );

      const layout = loadPersistedLayout();
      const usedPtyIds = new Set<string>();
      const restoredTabs: TerminalTab[] = [];

      if (layout) {
        for (const tab of layout.tabs) {
          if (state.tabs.some((t) => t.id === tab.id)) continue;
          const rootPane = rebuildPane(tab.rootPane, sessionIdByPtyId);
          if (!rootPane) continue;
          for (const leaf of collectLeaves(rootPane)) {
            const session = allSessions.find((s) => s.id === leaf.sessionId);
            if (session) usedPtyIds.add(session.ptyId);
          }
          const focusedPaneId = findLeaf(rootPane, tab.focusedPaneId)
            ? tab.focusedPaneId
            : getFirstLeaf(rootPane).id;
          restoredTabs.push({ ...tab, rootPane, focusedPaneId });
        }
      }

      const extraTabs: TerminalTab[] = toAdd
        .filter((s) => !usedPtyIds.has(s.ptyId))
        .map((s) => {
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

      const tabs = [...state.tabs, ...restoredTabs, ...extraTabs];
      const savedActiveTabId =
        layout?.activeTabId && tabs.some((t) => t.id === layout.activeTabId)
          ? layout.activeTabId
          : null;
      const activeTabId =
        state.activeTabId ?? savedActiveTabId ?? tabs[tabs.length - 1]?.id ?? null;
      const activeTab = tabs.find((t) => t.id === activeTabId);
      const activeSessionId =
        state.activeSessionId ??
        (activeTab
          ? findLeaf(activeTab.rootPane, activeTab.focusedPaneId)?.sessionId ??
            getFirstLeaf(activeTab.rootPane).sessionId
          : null);

      return { sessions: allSessions, tabs, activeTabId, activeSessionId };
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
        s.ptyId === ptyId
          ? { ...s, ...applyBusyTiming(s, updates) }
          : s,
      ),
    })),

  setSessionActivity: (id, activity) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id
          ? { ...s, ...applyBusyTiming(s, activity) }
          : s,
      ),
    })),

  setProcessRunning: (id, running) =>
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id
          ? {
              ...s,
              processRunning: running,
              ...applyBusyTiming(s, { isBusy: running, needsInput: s.needsInput }),
            }
          : s,
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
          createdAt: Date.now(),
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

  if (state.tabs !== prevState.tabs || state.activeTabId !== prevState.activeTabId) {
    persistLayout(state);
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
