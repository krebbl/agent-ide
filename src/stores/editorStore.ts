import { create } from "zustand";
import { invoke } from "../services/ipc";
import { nextActiveAfterClose } from "../utils/tabActivation";

export interface OpenFile {
  path: string;
  projectId: string;
  content: string;
  dirty: boolean;
}

export function languageFromPath(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "ts":
    case "mts":
    case "cts":
      return "typescript";
    case "tsx":
      return "typescript";
    case "js":
    case "mjs":
    case "cjs":
      return "javascript";
    case "jsx":
      return "javascript";
    case "json":
      return "json";
    case "css":
      return "css";
    case "scss":
      return "scss";
    case "less":
      return "less";
    case "html":
    case "htm":
      return "html";
    case "md":
      return "markdown";
    case "rs":
      return "rust";
    case "py":
      return "python";
    case "go":
      return "go";
    case "java":
      return "java";
    case "c":
    case "h":
      return "c";
    case "cpp":
    case "cc":
    case "cxx":
    case "hpp":
      return "cpp";
    case "toml":
      return "ini";
    case "yaml":
    case "yml":
      return "yaml";
    case "xml":
      return "xml";
    case "sh":
    case "bash":
    case "zsh":
      return "shell";
    case "sql":
      return "sql";
    case "lua":
      return "lua";
    case "rb":
      return "ruby";
    case "erb":
      return "erb";
    case "haml":
      return "haml";
    default:
      return "plaintext";
  }
}

export interface PendingReveal {
  path: string;
  line: number;
  column: number;
}

interface StoredTabs {
  openPaths: string[];
  activePath: string | null;
}

let tabsByWorktree: Record<string, StoredTabs> = {};
let tabsLoadedPromise: Promise<void> | null = null;
let persistTimer: ReturnType<typeof setTimeout> | null = null;

function ensureTabsLoaded(): Promise<void> {
  if (!tabsLoadedPromise) {
    tabsLoadedPromise = invoke<Record<string, StoredTabs>>("load_editor_tabs")
      .then((tabs) => {
        tabsByWorktree = tabs ?? {};
      })
      .catch(() => {});
  }
  return tabsLoadedPromise;
}

function schedulePersist() {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    invoke("save_editor_tabs", { tabs: tabsByWorktree }).catch(() => {});
  }, 300);
}

interface EditorState {
  openFiles: OpenFile[];
  activePath: string | null;
  worktreeKey: string | null;
  pendingReveal: PendingReveal | null;
  pendingClose: string | null;

  setWorktree: (key: string, projectId: string) => Promise<void>;
  openFile: (projectId: string, path: string) => Promise<void>;
  requestClose: (path: string) => void;
  confirmClose: (save: boolean) => Promise<void>;
  cancelClose: () => void;
  closeFile: (path: string) => void;
  closeUnderPath: (path: string) => void;
  remapPath: (oldPath: string, newPath: string) => void;
  closeAll: () => void;
  setActive: (path: string) => void;
  updateContent: (path: string, content: string) => void;
  saveFile: (path: string) => Promise<void>;
  saveActive: () => Promise<void>;
  setPendingReveal: (reveal: PendingReveal | null) => void;
}

export const useEditorStore = create<EditorState>((set, get) => ({
  openFiles: [],
  activePath: null,
  worktreeKey: null,
  pendingReveal: null,
  pendingClose: null,

  setWorktree: async (key, projectId) => {
    if (get().worktreeKey === key) return;
    await ensureTabsLoaded();
    const stored = tabsByWorktree[key];
    if (!stored || stored.openPaths.length === 0) {
      set({ worktreeKey: key, openFiles: [], activePath: null });
      return;
    }
    const files: OpenFile[] = [];
    for (const path of stored.openPaths) {
      try {
        const content = await invoke<string>("fs_read_file", { projectId, path });
        files.push({ path, projectId, content, dirty: false });
      } catch {
        // file no longer exists or is unreadable, skip
      }
    }
    if (files.length === 0) {
      // Every read failed — likely a transient failure (e.g. SSH not connected
      // yet), not genuinely missing files. Leave state and persisted tabs
      // untouched so a later setWorktree call can restore them.
      return;
    }
    const activePath =
      stored.activePath && files.some((f) => f.path === stored.activePath)
        ? stored.activePath
        : (files[files.length - 1]?.path ?? null);
    set({ worktreeKey: key, openFiles: files, activePath });
  },

  setPendingReveal: (reveal) => set({ pendingReveal: reveal }),

  requestClose: (path) => {
    const file = get().openFiles.find((f) => f.path === path);
    if (file?.dirty) {
      set({ pendingClose: path });
    } else {
      get().closeFile(path);
    }
  },

  confirmClose: async (save) => {
    const path = get().pendingClose;
    if (!path) return;
    set({ pendingClose: null });
    if (save) {
      try {
        await get().saveFile(path);
      } catch {
        return;
      }
    }
    get().closeFile(path);
  },

  cancelClose: () => set({ pendingClose: null }),

  closeUnderPath: (path) => {
    set((s) => {
      const isUnder = (p: string) => p === path || p.startsWith(path + "/");
      const openFiles = s.openFiles.filter((f) => !isUnder(f.path));
      const activePath =
        s.activePath && isUnder(s.activePath)
          ? (openFiles[openFiles.length - 1]?.path ?? null)
          : s.activePath;
      return { openFiles, activePath };
    });
  },

  remapPath: (oldPath, newPath) => {
    const remap = (p: string) =>
      p === oldPath || p.startsWith(oldPath + "/")
        ? newPath + p.slice(oldPath.length)
        : p;
    set((s) => ({
      openFiles: s.openFiles.map((f) =>
        f.path === remap(f.path) ? f : { ...f, path: remap(f.path) },
      ),
      activePath: s.activePath ? remap(s.activePath) : s.activePath,
    }));
  },
  openFile: async (projectId, path) => {
    const existing = get().openFiles.find((f) => f.path === path);
    if (existing) {
      set({ activePath: path });
      return;
    }
    const content = await invoke<string>("fs_read_file", { projectId, path });
    set((s) => ({
      openFiles: [...s.openFiles, { path, projectId, content, dirty: false }],
      activePath: path,
    }));
  },

  closeFile: (path) => {
    set((s) => ({
      openFiles: s.openFiles.filter((f) => f.path !== path),
      activePath: nextActiveAfterClose(
        s.openFiles.map((f) => f.path),
        path,
        s.activePath,
      ),
    }));
  },

  closeAll: () => set({ openFiles: [], activePath: null }),

  setActive: (path) => set({ activePath: path }),

  updateContent: (path, content) => {
    set((s) => ({
      openFiles: s.openFiles.map((f) =>
        f.path === path ? { ...f, content, dirty: true } : f,
      ),
    }));
  },

  saveFile: async (path) => {
    const file = get().openFiles.find((f) => f.path === path);
    if (!file || !file.dirty) return;
    await invoke("fs_write_file", {
      projectId: file.projectId,
      path: file.path,
      content: file.content,
    });
    set((s) => ({
      openFiles: s.openFiles.map((f) =>
        f.path === path ? { ...f, dirty: false } : f,
      ),
    }));
  },

  saveActive: async () => {
    const { activePath, saveFile } = get();
    if (activePath) await saveFile(activePath);
  },
}));

useEditorStore.subscribe((state, prev) => {
  if (state.openFiles === prev.openFiles && state.activePath === prev.activePath)
    return;
  if (!state.worktreeKey) return;
  tabsByWorktree[state.worktreeKey] = {
    openPaths: state.openFiles.map((f) => f.path),
    activePath: state.activePath,
  };
  schedulePersist();
});
