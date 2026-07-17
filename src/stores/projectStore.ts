import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { Project } from "../types";

interface ProjectStore {
  projects: Project[];
  activeProjectId: string | null;
  isLoading: boolean;
  error: string | null;
  loadProjects: () => Promise<void>;
  loadProjectsFromDisk: () => Promise<Project[]>;
  addProject: (project: Project) => Promise<void>;
  removeProject: (id: string) => Promise<void>;
  updateProject: (id: string, updates: Partial<Project>) => Promise<void>;
  setActiveProject: (id: string | null) => void;
  getActiveProject: () => Project | undefined;
}

export const useProjectStore = create<ProjectStore>((set, get) => ({
  projects: [],
  activeProjectId: null,
  isLoading: false,
  error: null,

  loadProjects: async () => {
    set({ isLoading: true, error: null });
    try {
      const projects = await get().loadProjectsFromDisk();
      set({ projects, isLoading: false });
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },

  loadProjectsFromDisk: async () => {
    const raw = await invoke("load_projects") as Array<Record<string, unknown>>;
    return raw.map((p) => {
      const conn = p.connection as Record<string, unknown>;
      return { ...p, type: conn.type } as unknown as Project;
    });
  },

  addProject: async (project: Project) => {
    const { projects, isLoading } = get();
    const current = isLoading || projects.length === 0
      ? await get().loadProjectsFromDisk()
      : projects;
    const updated = [...current, project];
    set({ projects: updated });
    try {
      await invoke("save_projects", { projects: updated });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removeProject: async (id: string) => {
    const { projects, activeProjectId, isLoading } = get();
    const current = isLoading || projects.length === 0
      ? await get().loadProjectsFromDisk()
      : projects;
    const project = current.find((p) => p.id === id);
    const updated = current.filter((p) => p.id !== id);
    set({
      projects: updated,
      activeProjectId: activeProjectId === id ? null : activeProjectId,
    });
    try {
      if (project && project.type === "ssh") {
        await invoke("ssh_delete_password", { projectId: id });
      }
      await invoke("save_projects", { projects: updated });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  updateProject: async (id: string, updates: Partial<Project>) => {
    const { projects, isLoading } = get();
    const current = isLoading || projects.length === 0
      ? await get().loadProjectsFromDisk()
      : projects;
    const updated = current.map((p) =>
      p.id === id ? { ...p, ...updates } : p,
    );
    set({ projects: updated });
    try {
      await invoke("save_projects", { projects: updated });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setActiveProject: (id: string | null) => {
    set({ activeProjectId: id });
  },

  getActiveProject: () => {
    const { projects, activeProjectId } = get();
    return projects.find((p) => p.id === activeProjectId);
  },
}));
