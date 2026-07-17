import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { Project } from "../types";

interface ProjectStore {
  projects: Project[];
  activeProjectId: string | null;
  isLoading: boolean;
  error: string | null;
  loadProjects: () => Promise<void>;
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
      const projects: Project[] = await invoke("load_projects");
      set({ projects, isLoading: false });
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },

  addProject: async (project: Project) => {
    const { projects } = get();
    const updated = [...projects, project];
    set({ projects: updated });
    try {
      await invoke("save_projects", { projects: updated });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removeProject: async (id: string) => {
    const { projects, activeProjectId } = get();
    const updated = projects.filter((p) => p.id !== id);
    set({
      projects: updated,
      activeProjectId: activeProjectId === id ? null : activeProjectId,
    });
    try {
      await invoke("save_projects", { projects: updated });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  updateProject: async (id: string, updates: Partial<Project>) => {
    const { projects } = get();
    const updated = projects.map((p) =>
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
