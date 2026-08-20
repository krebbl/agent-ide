import { invoke } from "./ipc";
import { ProcessInfo } from "../types";

export async function fetchSessionProcesses(
  ptyId: string,
): Promise<ProcessInfo[]> {
  return await invoke<ProcessInfo[]>("pty_session_processes", { sessionId: ptyId });
}