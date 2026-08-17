import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import type { IpcEvent } from "../ipc";

export async function invoke<T>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  return tauriInvoke<T>(command, payload);
}

export function listen<T>(
  event: string,
  handler: (event: IpcEvent<T>) => void,
): Promise<() => void> {
  return tauriListen<T>(event, (e) => handler({ payload: e.payload }));
}
