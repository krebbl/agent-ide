import { create } from "zustand";

export type FocusedZone = "editor" | "terminal" | null;
export type RightSidebarTab = "files" | "pr" | "jira";

interface UiState {
  rightSidebarVisible: boolean;
  toggleRightSidebar: () => void;
  rightSidebarTab: RightSidebarTab;
  setRightSidebarTab: (tab: RightSidebarTab) => void;
  focusedZone: FocusedZone;
  setFocusedZone: (zone: FocusedZone) => void;
  worktreeLoading: boolean;
  setWorktreeLoading: (loading: boolean) => void;
  fileSearchOpen: boolean;
  setFileSearchOpen: (open: boolean) => void;
  projectSettingsProjectId: string | null;
  openProjectSettings: (projectId: string) => void;
  closeProjectSettings: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  rightSidebarVisible: true,
  toggleRightSidebar: () =>
    set((s) => ({ rightSidebarVisible: !s.rightSidebarVisible })),
  rightSidebarTab: "files",
  setRightSidebarTab: (tab) => set({ rightSidebarTab: tab }),
  focusedZone: null,
  setFocusedZone: (zone) => set({ focusedZone: zone }),
  worktreeLoading: false,
  setWorktreeLoading: (loading) => set({ worktreeLoading: loading }),
  fileSearchOpen: false,
  setFileSearchOpen: (open) => set({ fileSearchOpen: open }),
  projectSettingsProjectId: null,
  openProjectSettings: (projectId) => set({ projectSettingsProjectId: projectId }),
  closeProjectSettings: () => set({ projectSettingsProjectId: null }),
}));
