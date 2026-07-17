import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";

export type ConnectionStatus = "connected" | "disconnected" | "reconnecting" | "error";

interface ConnectionStatusEntry {
  status: ConnectionStatus;
  error?: string;
}

interface ConnectionStatusStore {
  statuses: Record<string, ConnectionStatusEntry>;
  setStatus: (projectId: string, status: ConnectionStatus, error?: string) => void;
  clearStatus: (projectId: string) => void;
}

export const useConnectionStatusStore = create<ConnectionStatusStore>((set) => ({
  statuses: {},
  setStatus: (projectId, status, error) =>
    set((state) => ({
      statuses: { ...state.statuses, [projectId]: { status, error } },
    })),
  clearStatus: (projectId) =>
    set((state) => {
      const next = { ...state.statuses };
      delete next[projectId];
      return { statuses: next };
    }),
}));

export function initConnectionStatusListener() {
  listen<{ project_id: string; status: string; error?: string }>(
    "ssh_connection_status",
    (event) => {
      const { project_id, status, error } = event.payload;
      useConnectionStatusStore.getState().setStatus(project_id, status as ConnectionStatus, error);
    },
  );
}
