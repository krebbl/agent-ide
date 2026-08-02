import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface LspServerInfo {
  project_id: string;
  language_id: string;
  status: string;
  capabilities: unknown;
  server_info: unknown;
}

export interface LspMessageEvent {
  project_id: string;
  language_id: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  message: any;
}

export interface LspStatusEvent {
  project_id: string;
  language_id: string;
  status: string;
  error: string | null;
}

export function lspStart(
  projectId: string,
  languageId: string,
  rootPath: string,
): Promise<LspServerInfo> {
  return invoke<LspServerInfo>("lsp_start", { projectId, languageId, rootPath });
}

export function lspRequest<T = unknown>(
  projectId: string,
  languageId: string,
  method: string,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  params: any,
): Promise<T> {
  return invoke<T>("lsp_request", { projectId, languageId, method, params });
}

export function lspNotify(
  projectId: string,
  languageId: string,
  method: string,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  params: any,
): Promise<void> {
  return invoke<void>("lsp_notify", { projectId, languageId, method, params });
}

export function lspStop(projectId: string, languageId: string): Promise<void> {
  return invoke<void>("lsp_stop", { projectId, languageId });
}

export function lspServerAvailable(
  languageId: string,
  projectId: string,
): Promise<boolean> {
  return invoke<boolean>("lsp_server_available", { languageId, projectId });
}

export function onLspMessage(handler: (event: LspMessageEvent) => void) {
  return listen<LspMessageEvent>("lsp://message", (e) => handler(e.payload));
}

export function onLspStatus(handler: (event: LspStatusEvent) => void) {
  return listen<LspStatusEvent>("lsp://status", (e) => handler(e.payload));
}
