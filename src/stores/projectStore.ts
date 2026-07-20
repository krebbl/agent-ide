import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { Project, Worktree } from "../types";

interface ProjectStore {
  projects: Project[];
  activeProjectId: string | null;
  selectedWorktreeId: string | null;
  isLoading: boolean;
  error: string | null;
  worktreeLoading: Record<string, boolean>;
  loadProjects: () => Promise<void>;
  loadProjectsFromDisk: () => Promise<Project[]>;
  addProject: (project: Project) => Promise<void>;
  removeProject: (id: string) => Promise<void>;
  updateProject: (id: string, updates: Partial<Project>) => Promise<void>;
  setActiveProject: (id: string | null) => void;
  getActiveProject: () => Project | undefined;
  fetchWorktrees: (projectId: string) => Promise<void>;
  setActiveWorktree: (projectId: string, worktreeId: string) => Promise<void>;
  removeWorktree: (projectId: string, worktreePath: string, force?: boolean) => Promise<void>;
  refreshWorktrees: (projectId: string) => Promise<void>;
  addWorktree: (projectId: string, branch: string, name: string, newBranch: boolean) => Promise<void>;
}

export const useProjectStore = create<ProjectStore>((set, get) => ({
  projects: [],
  activeProjectId: null,
  selectedWorktreeId: null,
  isLoading: false,
  error: null,
  worktreeLoading: {},

  loadProjects: async () => {
    set({ isLoading: true, error: null });
    try {
      const loadedProjects = await get().loadProjectsFromDisk();
      let activeProjectId: string | null = null;
      let selectedWorktreeId: string | null = null;
      let foundSelected = false;
      const projects = loadedProjects.map((p) => {
        if (!foundSelected && p.activeWorktreeId) {
          foundSelected = true;
          activeProjectId = p.id;
          selectedWorktreeId = p.activeWorktreeId;
          return p;
        }
        if (p.activeWorktreeId) {
          return { ...p, activeWorktreeId: null };
        }
        return p;
      });
      set({ projects, isLoading: false, activeProjectId, selectedWorktreeId });
      if (foundSelected) {
        await invoke("save_projects", { projects }).catch(() => {});
      }

      for (const project of projects) {
        if (project.type === "ssh") {
          const conn = project.connection as { host: string; port: number; username: string; authMethod: string; keyPath?: string };
          const password = await invoke<string | null>("ssh_get_password", { projectId: project.id }).catch(() => null);
          invoke("ssh_connect", {
            projectId: project.id,
            host: conn.host,
            port: conn.port,
            username: conn.username,
            authMethod: conn.authMethod,
            keyPath: conn.keyPath ?? null,
            password: password ?? null,
          }).catch(() => {});
        }
      }
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
      if (project.type === "ssh") {
        const conn = project.connection as { host: string; port: number; username: string; authMethod: string; keyPath?: string };
        const password = await invoke<string | null>("ssh_get_password", { projectId: project.id }).catch(() => null);
        invoke("ssh_connect", {
          projectId: project.id,
          host: conn.host,
          port: conn.port,
          username: conn.username,
          authMethod: conn.authMethod,
          keyPath: conn.keyPath ?? null,
          password: password ?? null,
        }).catch(() => {});
      }
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
    const activeCleared = activeProjectId === id;
    set({
      projects: updated,
      activeProjectId: activeCleared ? null : activeProjectId,
      selectedWorktreeId: activeCleared ? null : get().selectedWorktreeId,
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
    const { projects } = get();
    if (!id) {
      const updated = projects.map((p) => ({ ...p, activeWorktreeId: null }));
      set({ projects: updated, activeProjectId: null, selectedWorktreeId: null });
      invoke("save_projects", { projects: updated }).catch(() => {});
      return;
    }
    const target = projects.find((p) => p.id === id);
    const worktreeId = target?.activeWorktreeId ?? null;
    const updated = projects.map((p) =>
      p.id === id ? p : { ...p, activeWorktreeId: null },
    );
    set({ projects: updated, activeProjectId: id, selectedWorktreeId: worktreeId });
    invoke("save_projects", { projects: updated }).catch(() => {});
  },

  getActiveProject: () => {
    const { projects, activeProjectId } = get();
    return projects.find((p) => p.id === activeProjectId);
  },

  fetchWorktrees: async (projectId: string) => {
    set((s) => ({ worktreeLoading: { ...s.worktreeLoading, [projectId]: true } }));
    try {
      const raw = await invoke<Worktree[]>("git_worktree_list_async", { projectId });
      const worktrees: Worktree[] = raw.map((wt) => ({
        id: wt.id,
        branch: wt.branch,
        path: wt.path,
        isMain: wt.isMain,
        status: wt.status as "clean" | "dirty" | "unknown",
        ahead: wt.ahead,
        behind: wt.behind,
      }));
      set((s) => {
        const selectedWorktreeId = s.selectedWorktreeId;
        const activeProjectId = s.activeProjectId;
        const updated = s.projects.map((p) => {
          if (p.id !== projectId) return p;
          const activeWorktreeId =
            p.activeWorktreeId && worktrees.some((w) => w.id === p.activeWorktreeId)
              ? p.activeWorktreeId
              : null;
          return { ...p, worktrees, activeWorktreeId };
        });
        const activeRemoved =
          activeProjectId === projectId &&
          selectedWorktreeId !== null &&
          !worktrees.some((w) => w.id === selectedWorktreeId);
        return {
          projects: updated,
          selectedWorktreeId: activeRemoved ? null : selectedWorktreeId,
        };
      });
      const projects = get().projects;
      await invoke("save_projects", { projects });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set((s) => ({ worktreeLoading: { ...s.worktreeLoading, [projectId]: false } }));
    }
  },

  setActiveWorktree: async (projectId: string, worktreeId: string) => {
    const { projects, isLoading } = get();
    const current = isLoading || projects.length === 0
      ? await get().loadProjectsFromDisk()
      : projects;
    const updated = current.map((p) =>
      p.id === projectId
        ? { ...p, activeWorktreeId: worktreeId }
        : { ...p, activeWorktreeId: null },
    );
    set({ projects: updated, activeProjectId: projectId, selectedWorktreeId: worktreeId });
    try {
      await invoke("save_projects", { projects: updated });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  addWorktree: async (projectId: string, branch: string, name: string, newBranch: boolean) => {
    await invoke("git_worktree_add_async", { projectId, branch, name, newBranch });
    await get().refreshWorktrees(projectId);
  },

  removeWorktree: async (projectId: string, worktreePath: string, force = false) => {
    await invoke("git_worktree_remove_async", { projectId, worktreePath, force });
    await get().refreshWorktrees(projectId);
  },

  refreshWorktrees: async (projectId: string) => {
    await get().fetchWorktrees(projectId);
  },
}));
