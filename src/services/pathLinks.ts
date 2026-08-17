import { invoke } from "./ipc";
import { monaco } from "../utils/monacoSetup";
import { useEditorStore, type OpenFile } from "../stores/editorStore";
import { useFileTreeStore } from "../stores/fileTreeStore";

const LANGUAGES = [
  "html",
  "css",
  "scss",
  "less",
  "markdown",
  "plaintext",
  "xml",
  "json",
  "yaml",
];

function fileForModel(model: monaco.editor.ITextModel): OpenFile | undefined {
  const path = model.uri.path;
  return useEditorStore.getState().openFiles.find((f) => f.path === path);
}

function resolvePath(baseDir: string, relative: string): string {
  const parts = relative.startsWith("/")
    ? []
    : baseDir.split("/").filter(Boolean);
  for (const segment of relative.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") parts.pop();
    else parts.push(segment);
  }
  return "/" + parts.join("/");
}

function extractPathAt(line: string, index: number): string | null {
  const isBoundary = (ch: string) => /[\s"'`()<>\[\]{}=]/.test(ch);
  let start = index;
  let end = index;
  while (start > 0 && !isBoundary(line[start - 1])) start--;
  while (end < line.length && !isBoundary(line[end])) end++;
  let token = line.slice(start, end);
  token = token.split(/[?#]/)[0];
  if (!token || token.includes("://")) return null;
  if (!token.includes("/")) return null;
  if (!/\.[A-Za-z0-9]{1,10}$/.test(token)) return null;
  return token;
}

let installed = false;

export function installPathLinkProviders() {
  if (installed) return;
  installed = true;

  for (const language of LANGUAGES) {
    monaco.languages.registerDefinitionProvider(language, {
      provideDefinition: async (model, position) => {
        const file = fileForModel(model);
        if (!file) return null;
        const line = model.getLineContent(position.lineNumber);
        const token = extractPathAt(line, position.column - 1);
        if (!token) return null;
        const baseDir = file.path.split("/").slice(0, -1).join("/");
        const candidates = [resolvePath(baseDir, token)];
        if (token.startsWith("/")) {
          const rootPath = useFileTreeStore.getState().rootPath;
          if (rootPath) candidates.push(rootPath + token);
        }
        for (const resolved of candidates) {
          if (resolved === file.path) continue;
          const exists = await invoke<boolean>("fs_exists", {
            projectId: file.projectId,
            path: resolved,
          });
          if (!exists) continue;
          return [
            {
              uri: monaco.Uri.parse(resolved),
              range: new monaco.Range(1, 1, 1, 1),
            },
          ];
        }
        return null;
      },
    });
  }
}
