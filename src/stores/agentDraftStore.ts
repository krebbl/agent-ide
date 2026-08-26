import { create } from "zustand";
import { AgentId } from "../types";

// Temporary in-memory drafts of the "Start Agent Session" form, keyed per
// project. Kept so that canceling/escaping the dialog (which unmounts it)
// preserves the user's input for the next time the dialog is opened in the
// same project. Never persisted to disk.
export interface AgentSessionDraft {
  prompt: string;
  selectedAgentId: AgentId | "";
  selectedModel: string;
  selectedBranch: string;
  worktreeName: string;
  worktreeNameDirty: boolean;
  createNew: boolean;
  setupCommand: string;
}

export const EMPTY_DRAFT: AgentSessionDraft = {
  prompt: "",
  selectedAgentId: "",
  selectedModel: "",
  selectedBranch: "",
  worktreeName: "",
  worktreeNameDirty: false,
  createNew: false,
  setupCommand: "",
};

interface AgentDraftStore {
  drafts: Record<string, AgentSessionDraft>;
  update: (projectId: string, patch: Partial<AgentSessionDraft>) => void;
  clear: (projectId: string) => void;
}

export const useAgentDraftStore = create<AgentDraftStore>((set) => ({
  drafts: {},
  update: (projectId, patch) =>
    set((s) => ({
      drafts: {
        ...s.drafts,
        [projectId]: { ...(s.drafts[projectId] ?? EMPTY_DRAFT), ...patch },
      },
    })),
  clear: (projectId) =>
    set((s) => {
      const { [projectId]: _removed, ...rest } = s.drafts;
      return { drafts: rest };
    }),
}));
