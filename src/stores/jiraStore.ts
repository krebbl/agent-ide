import { create } from "zustand";
import { invoke } from "../services/ipc";
import { JiraConfigState, JiraConfigUpdate, JiraIssueResult } from "../types";

type JiraCacheEntry = {
  result: JiraIssueResult | null;
  loading: boolean;
};

interface JiraStore {
  cache: Record<string, JiraCacheEntry>;
  configs: Record<string, JiraConfigState>;
  loadConfig: (projectId: string) => Promise<void>;
  saveConfig: (
    projectId: string,
    siteUrl: string,
    email: string,
    apiToken?: string,
  ) => Promise<JiraConfigUpdate>;
  fetchIssueForBranch: (projectId: string, branch: string, force?: boolean) => Promise<void>;
  getEntry: (projectId: string, branch: string) => JiraCacheEntry | undefined;
}

export const useJiraStore = create<JiraStore>((set, get) => ({
  cache: {},
  configs: {},

  loadConfig: async (projectId) => {
    if (get().configs[projectId]) return;
    const config = await invoke<JiraConfigState>("jira_get_config", { projectId });
    set((s) => ({ configs: { ...s.configs, [projectId]: config } }));
  },

  saveConfig: async (projectId, siteUrl, email, apiToken) => {
    const update = await invoke<JiraConfigUpdate>("jira_set_config", {
      projectId,
      siteUrl,
      email,
      apiToken: apiToken ?? null,
    });
    set((s) => ({
      configs: { ...s.configs, [projectId]: update.config },
      cache: {},
    }));
    return update;
  },

  fetchIssueForBranch: async (projectId, branch, force = false) => {
    const key = `${projectId}:${branch}`;
    const existing = get().cache[key];
    if (existing?.loading) return;
    if (existing?.result && !force) return;

    set((s) => ({ cache: { ...s.cache, [key]: { result: s.cache[key]?.result ?? null, loading: true } } }));

    try {
      const result = await invoke<JiraIssueResult>("jira_issue_for_branch", { projectId, branch });
      set((s) => ({ cache: { ...s.cache, [key]: { result, loading: false } } }));
    } catch (e) {
      const result: JiraIssueResult = {
        issue: null,
        ticketKey: null,
        error: e instanceof Error ? e.message : String(e),
      };
      set((s) => ({ cache: { ...s.cache, [key]: { result, loading: false } } }));
    }
  },

  getEntry: (projectId, branch) => get().cache[`${projectId}:${branch}`],
}));
