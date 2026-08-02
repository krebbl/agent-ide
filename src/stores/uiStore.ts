import { create } from "zustand";

export type FocusedZone = "editor" | "terminal" | null;

interface UiState {
  rightSidebarVisible: boolean;
  toggleRightSidebar: () => void;
  focusedZone: FocusedZone;
  setFocusedZone: (zone: FocusedZone) => void;
}

export const useUiStore = create<UiState>((set) => ({
  rightSidebarVisible: true,
  toggleRightSidebar: () =>
    set((s) => ({ rightSidebarVisible: !s.rightSidebarVisible })),
  focusedZone: null,
  setFocusedZone: (zone) => set({ focusedZone: zone }),
}));
