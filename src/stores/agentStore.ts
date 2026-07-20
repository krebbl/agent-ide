import { create } from "zustand";
import { AgentId, AgentStatus } from "../types";
import { checkAgentReady, checkAgentsReady } from "../services/agents";

interface AgentStore {
  agents: AgentStatus[];
  isLoading: boolean;
  error: string | null;
  checkAll: () => Promise<void>;
  checkById: (id: AgentId) => Promise<AgentStatus>;
  getInstalled: () => AgentStatus[];
  getById: (id: AgentId) => AgentStatus | undefined;
}

export const useAgentStore = create<AgentStore>((set, get) => ({
  agents: [],
  isLoading: false,
  error: null,

  checkAll: async () => {
    set({ isLoading: true, error: null });
    try {
      const agents = await checkAgentsReady();
      set({ agents, isLoading: false });
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },

  checkById: async (id) => {
    const status = await checkAgentReady(id);
    set((state) => ({
      agents: state.agents.map((a) => (a.id === id ? status : a)),
    }));
    return status;
  },

  getInstalled: () => {
    return get().agents.filter((a) => a.installed);
  },

  getById: (id) => {
    return get().agents.find((a) => a.id === id);
  },
}));
