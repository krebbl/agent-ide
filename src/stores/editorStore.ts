import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

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
    default:
      return "plaintext";
  }
}

interface EditorState {
  openFiles: OpenFile[];
  activePath: string | null;

  openFile: (projectId: string, path: string) => Promise<void>;
  closeFile: (path: string) => void;
  closeAll: () => void;
  setActive: (path: string) => void;
  updateContent: (path: string, content: string) => void;
  saveFile: (path: string) => Promise<void>;
  saveActive: () => Promise<void>;
}

export const useEditorStore = create<EditorState>((set, get) => ({
  openFiles: [],
  activePath: null,

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
    set((s) => {
      const idx = s.openFiles.findIndex((f) => f.path === path);
      const openFiles = s.openFiles.filter((f) => f.path !== path);
      let activePath = s.activePath;
      if (activePath === path) {
        const neighbor = openFiles[idx] ?? openFiles[idx - 1] ?? null;
        activePath = neighbor?.path ?? null;
      }
      return { openFiles, activePath };
    });
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
