import { create } from "zustand";

export type LspServerStatus =
  | "starting"
  | "ready"
  | "stopped"
  | "crashed"
  | "unavailable";

interface LspState {
  servers: Record<string, { status: LspServerStatus; error: string | null }>;
  setStatus: (key: string, status: LspServerStatus, error?: string | null) => void;
}

export const useLspStore = create<LspState>((set) => ({
  servers: {},
  setStatus: (key, status, error = null) =>
    set((s) => ({
      servers: { ...s.servers, [key]: { status, error } },
    })),
}));
