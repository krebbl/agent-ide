import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

/** Rust DirEntry serialized as camelCase: `isDir` */
export interface DirEntry {
  name: string;
  isDir: boolean;
  size: number;
}

interface FileTreeState {
  nodeState: {
    [dirPath: string]: {
      expanded: boolean;
      children: DirEntry[];
      loading: boolean;
      error: string | null;
    };
  };
  ignoredFiles: string[];
  showIgnored: boolean;
  rootPath: string | null;
  projectId: string | null;
  projectType: "local" | "ssh" | null;

  setRoot: (rootPath: string, projectId: string, projectType: "local" | "ssh") => Promise<void>;
  toggleDir: (dirPath: string) => Promise<void>;
  refreshDir: (dirPath: string) => Promise<void>;
  setShowIgnored: (show: boolean) => void;
}

async function loadGitignorePatterns(
  rootPath: string,
  projectId: string,
): Promise<string[]> {
  try {
    const content = await invoke<string>("fs_read_file", {
      projectId,
      path: `${rootPath}/.gitignore`,
    });
    const patterns = content
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith("#") && !l.startsWith("!"));
    const dirs = await invoke<DirEntry[]>("fs_read_dir", { projectId, path: rootPath });
    const ignored: string[] = [];
    for (const entry of dirs) {
      for (const pattern of patterns) {
        if (gitignoreMatch(entry.name, pattern)) {
          ignored.push(entry.name);
          break;
        }
      }
    }
    return ignored;
  } catch {
    return [];
  }
}

function gitignoreMatch(filename: string, pattern: string): boolean {
  if (pattern.endsWith("/")) return false;
  if (pattern.includes("*")) {
    const regex = new RegExp(
      `^${pattern.replace(/\./g, "\\.").replace(/\*/g, ".*").replace(/\?/g, ".")}$`,
    );
    return regex.test(filename);
  }
  return filename === pattern;
}

async function fetchDirEntries(
  dirPath: string,
  projectId: string,
  ignoredFiles: string[],
  showIgnored: boolean,
): Promise<DirEntry[]> {
  const raw = await invoke<DirEntry[]>("fs_read_dir", { projectId, path: dirPath });
  if (showIgnored) return raw;
  return raw.filter((e) => !ignoredFiles.includes(e.name));
}

export const useFileTreeStore = create<FileTreeState>((set, get) => ({
  nodeState: {},
  ignoredFiles: [],
  showIgnored: false,
  rootPath: null,
  projectId: null,
  projectType: null,

  setRoot: async (rootPath: string, projectId: string) => {
    const cur = get();
    if (cur.rootPath === rootPath && cur.projectId === projectId) return;
    const ignoredFiles = await loadGitignorePatterns(rootPath, projectId);
    set({ rootPath, projectId, ignoredFiles, nodeState: {} });
    await get().toggleDir(rootPath);
  },

  toggleDir: async (dirPath) => {
    const state = get().nodeState[dirPath];
    if (state?.loading) return;

    if (state?.expanded) {
      set((s) => ({
        nodeState: {
          ...s.nodeState,
          [dirPath]: { ...s.nodeState[dirPath]!, expanded: false },
        },
      }));
      return;
    }

    set((s) => ({
      nodeState: {
        ...s.nodeState,
        [dirPath]: { expanded: true, children: state?.children ?? [], loading: true, error: null },
      },
    }));

    try {
      const { projectId, ignoredFiles, showIgnored } = get();
      if (!projectId) return;
      const children = await fetchDirEntries(dirPath, projectId, ignoredFiles, showIgnored);
      children.sort((a, b) => Number(b.isDir) - Number(a.isDir) || a.name.localeCompare(b.name));
      set((s) => ({
        nodeState: {
          ...s.nodeState,
          [dirPath]: { expanded: true, children, loading: false, error: null },
        },
      }));
    } catch (e) {
      set((s) => ({
        nodeState: {
          ...s.nodeState,
          [dirPath]: { expanded: false, children: [], loading: false, error: String(e) },
        },
      }));
    }
  },

  refreshDir: async (dirPath) => {
    const curState = get().nodeState[dirPath];
    if (curState?.loading) return;

    set((s) => ({
      nodeState: {
        ...s.nodeState,
        [dirPath]: { ...curState!, loading: true, error: null },
      },
    }));

    try {
      const { projectId: projId, ignoredFiles, showIgnored } = get();
      if (!projId) return;
      const children = await fetchDirEntries(dirPath, projId, ignoredFiles, showIgnored);
      children.sort((a, b) => Number(b.isDir) - Number(a.isDir) || a.name.localeCompare(b.name));
      set((s) => ({
        nodeState: { ...s.nodeState, [dirPath]: { expanded: true, children, loading: false, error: null } },
      }));
    } catch (e) {
      set((s) => ({
        nodeState: { ...s.nodeState, [dirPath]: { ...curState!, loading: false, error: String(e) } },
      }));
    }
  },

  setShowIgnored: async (show) => {
    set({ showIgnored: show });
    const { nodeState, projectId: projId, ignoredFiles } = get();
    if (!projId) return;
    const expandedDirs = Object.entries(nodeState)
      .filter(([, v]) => v.expanded)
      .map(([k]) => k);

    for (const dir of expandedDirs) {
      try {
        const reloaded = await fetchDirEntries(dir, projId, ignoredFiles, show);
        reloaded.sort((a, b) => Number(b.isDir) - Number(a.isDir) || a.name.localeCompare(b.name));
        set((s) => ({
          nodeState: { ...s.nodeState, [dir]: { expanded: true, children: reloaded, loading: false, error: null } },
        }));
      } catch {
        // ignore — keep existing children
      }
    }
  },
}));
