import { create } from "zustand";

interface UiState {
  rightSidebarVisible: boolean;
  toggleRightSidebar: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  rightSidebarVisible: true,
  toggleRightSidebar: () =>
    set((s) => ({ rightSidebarVisible: !s.rightSidebarVisible })),
}));
