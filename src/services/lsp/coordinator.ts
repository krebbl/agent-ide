import type { editor } from "monaco-editor";
import { monaco } from "../../utils/monacoSetup";
import { useEditorStore, type OpenFile } from "../../stores/editorStore";
import { useFileTreeStore } from "../../stores/fileTreeStore";
import { useLspStore, type LspServerStatus } from "../../stores/lspStore";
import * as client from "./client";
import { pathToUri, uriToPath, toMarkers } from "./converters";

const SERVER_KEY_BY_LSP_LANGUAGE: Record<string, string> = {
  typescript: "typescript",
  typescriptreact: "typescript",
  javascript: "typescript",
  javascriptreact: "typescript",
  rust: "rust",
  python: "python",
  go: "go",
  c: "c",
  cpp: "cpp",
};

export function lspLanguageFromPath(path: string): string | null {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "ts":
    case "mts":
    case "cts":
      return "typescript";
    case "tsx":
      return "typescriptreact";
    case "js":
    case "mjs":
    case "cjs":
      return "javascript";
    case "jsx":
      return "javascriptreact";
    case "rs":
      return "rust";
    case "py":
      return "python";
    case "go":
      return "go";
    case "c":
    case "h":
      return "c";
    case "cpp":
    case "cc":
    case "cxx":
    case "hpp":
      return "cpp";
    default:
      return null;
  }
}

export function serverKeyForMonacoLanguage(monacoLanguage: string): string | null {
  return SERVER_KEY_BY_LSP_LANGUAGE[monacoLanguage] ?? null;
}

function serverKeyForPath(path: string): string | null {
  const lspLanguage = lspLanguageFromPath(path);
  return lspLanguage ? (SERVER_KEY_BY_LSP_LANGUAGE[lspLanguage] ?? null) : null;
}

export const FileChangeType = {
  Created: 1,
  Changed: 2,
  Deleted: 3,
} as const;

const serverPromises = new Map<string, Promise<boolean>>();
const startedServers = new Set<string>();

export function ensureServer(projectId: string, serverKey: string): Promise<boolean> {
  const key = `${projectId}:${serverKey}`;
  let promise = serverPromises.get(key);
  if (!promise) {
    promise = (async () => {
      const available = await client.lspServerAvailable(serverKey);
      if (!available) {
        useLspStore.getState().setStatus(key, "unavailable");
        return false;
      }
      const rootPath = useFileTreeStore.getState().rootPath;
      if (!rootPath) return false;
      try {
        await client.lspStart(projectId, serverKey, rootPath);
        startedServers.add(key);
        return true;
      } catch (e) {
        useLspStore.getState().setStatus(key, "crashed", String(e));
        return false;
      }
    })();
    serverPromises.set(key, promise);
    promise.then((ok) => {
      if (!ok) {
        serverPromises.delete(key);
        startedServers.delete(key);
      }
    });
  }
  return promise;
}

export async function restartServer(
  projectId: string,
  serverKey: string,
): Promise<void> {
  const key = `${projectId}:${serverKey}`;
  serverPromises.delete(key);
  startedServers.delete(key);
  await client.lspStop(projectId, serverKey).catch(() => undefined);
  await ensureServer(projectId, serverKey);
}

export function notifyWatchedFiles(
  projectId: string,
  changes: { path: string; type: number }[],
) {
  for (const key of startedServers) {
    if (!key.startsWith(projectId + ":")) continue;
    const serverKey = key.slice(projectId.length + 1);
    void client
      .lspNotify(projectId, serverKey, "workspace/didChangeWatchedFiles", {
        changes: changes.map((c) => ({ uri: pathToUri(c.path), type: c.type })),
      })
      .catch(() => undefined);
  }
}

const queues = new Map<string, Promise<unknown>>();
const versions = new Map<string, number>();

function enqueue<T>(path: string, task: () => Promise<T>): Promise<T> {
  const prev = queues.get(path) ?? Promise.resolve();
  const next = prev.catch(() => undefined).then(task);
  queues.set(
    path,
    next.catch(() => undefined),
  );
  return next;
}

export function lspDocumentRequest<T>(
  file: OpenFile,
  method: string,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  params: any,
): Promise<T | null> {
  const serverKey = serverKeyForPath(file.path);
  if (!serverKey) return Promise.resolve(null);
  return enqueue(file.path, async () => {
    if (!(await ensureServer(file.projectId, serverKey))) return null;
    return client.lspRequest<T>(file.projectId, serverKey, method, params);
  });
}

function didOpen(file: OpenFile) {
  const lspLanguage = lspLanguageFromPath(file.path);
  const serverKey = serverKeyForPath(file.path);
  if (!lspLanguage || !serverKey) return;
  const text = file.content;
  versions.set(file.path, 1);
  enqueue(file.path, async () => {
    if (!(await ensureServer(file.projectId, serverKey))) return;
    await client.lspNotify(file.projectId, serverKey, "textDocument/didOpen", {
      textDocument: {
        uri: pathToUri(file.path),
        languageId: lspLanguage,
        version: 1,
        text,
      },
    });
  });
}

export function contentChanged(
  projectId: string,
  path: string,
  changes: editor.IModelContentChange[],
) {
  const serverKey = serverKeyForPath(path);
  if (!serverKey || !versions.has(path)) return;
  const version = (versions.get(path) ?? 1) + 1;
  versions.set(path, version);
  enqueue(path, async () => {
    if (!(await ensureServer(projectId, serverKey))) return;
    await client.lspNotify(projectId, serverKey, "textDocument/didChange", {
      textDocument: { uri: pathToUri(path), version },
      contentChanges: changes.map((c) => ({
        range: {
          start: { line: c.range.startLineNumber - 1, character: c.range.startColumn - 1 },
          end: { line: c.range.endLineNumber - 1, character: c.range.endColumn - 1 },
        },
        rangeLength: c.rangeLength,
        text: c.text,
      })),
    });
  });
}

function didSave(file: OpenFile) {
  const serverKey = serverKeyForPath(file.path);
  if (!serverKey || !versions.has(file.path)) return;
  enqueue(file.path, async () => {
    if (!(await ensureServer(file.projectId, serverKey))) return;
    await client.lspNotify(file.projectId, serverKey, "textDocument/didSave", {
      textDocument: { uri: pathToUri(file.path) },
    });
  });
}

function didClose(file: OpenFile) {
  const serverKey = serverKeyForPath(file.path);
  if (!serverKey || !versions.has(file.path)) return;
  enqueue(file.path, async () => {
    if (await ensureServer(file.projectId, serverKey)) {
      await client.lspNotify(file.projectId, serverKey, "textDocument/didClose", {
        textDocument: { uri: pathToUri(file.path) },
      });
    }
    versions.delete(file.path);
    queues.delete(file.path);
  });
}

let installed = false;

export function installLsp() {
  if (installed) return;
  installed = true;

  client.onLspMessage((event) => {
    const msg = event.message;
    if (msg?.method === "textDocument/publishDiagnostics" && msg.params) {
      const path = uriToPath(msg.params.uri);
      const model = monaco.editor
        .getModels()
        .find((m) => m.uri.path === path);
      if (model) {
        monaco.editor.setModelMarkers(
          model,
          "lsp",
          toMarkers(msg.params.diagnostics ?? []),
        );
      }
    }
  });

  client.onLspStatus((event) => {
    const key = `${event.project_id}:${event.language_id}`;
    useLspStore
      .getState()
      .setStatus(key, event.status as LspServerStatus, event.error);
    if (event.status === "crashed" || event.status === "stopped") {
      serverPromises.delete(key);
      startedServers.delete(key);
    }
  });

  useEditorStore.subscribe((state, prev) => {
    for (const file of state.openFiles) {
      if (!prev.openFiles.some((f) => f.path === file.path)) {
        didOpen(file);
      }
      const prevFile = prev.openFiles.find((f) => f.path === file.path);
      if (prevFile?.dirty && !file.dirty) {
        didSave(file);
      }
    }
    for (const file of prev.openFiles) {
      if (!state.openFiles.some((f) => f.path === file.path)) {
        didClose(file);
      }
    }
  });
}
