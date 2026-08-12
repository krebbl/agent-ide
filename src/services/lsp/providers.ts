import { monaco } from "../../utils/monacoSetup";
import { useEditorStore, type OpenFile } from "../../stores/editorStore";
import { lspDocumentRequest } from "./coordinator";
import {
  pathToUri,
  toLspPosition,
  toMonacoRange,
  toCompletionItem,
  hoverContents,
  toLocations,
  toWorkspaceEdit,
  toDocumentSymbols,
} from "./converters";

/* eslint-disable @typescript-eslint/no-explicit-any */

const MONACO_LANGUAGES = [
  "typescript",
  "javascript",
  "rust",
  "python",
  "go",
  "ruby",
  "c",
  "cpp",
];

function fileForModel(model: monaco.editor.ITextModel): OpenFile | undefined {
  const path = model.uri.path;
  return useEditorStore.getState().openFiles.find((f) => f.path === path);
}

function textDocumentPosition(file: OpenFile, position: monaco.Position) {
  return {
    textDocument: { uri: pathToUri(file.path) },
    position: toLspPosition(position),
  };
}

let registered = false;

export function registerProviders() {
  if (registered) return;
  registered = true;

  for (const language of MONACO_LANGUAGES) {
    monaco.languages.registerCompletionItemProvider(language, {
      triggerCharacters: [".", '"', "'", "/", "@", "<"],
      provideCompletionItems: async (model, position) => {
        const file = fileForModel(model);
        if (!file) return { suggestions: [] };
        const result = await lspDocumentRequest<any>(
          file,
          "textDocument/completion",
          {
            ...textDocumentPosition(file, position),
            context: { triggerKind: 1 },
          },
        );
        if (!result) return { suggestions: [] };
        const items = Array.isArray(result) ? result : (result.items ?? []);
        const word = model.getWordUntilPosition(position);
        const fallbackRange = new monaco.Range(
          position.lineNumber,
          word.startColumn,
          position.lineNumber,
          word.endColumn,
        );
        return {
          suggestions: items.map((item: any) =>
            toCompletionItem(item, fallbackRange),
          ),
        };
      },
    });

    monaco.languages.registerHoverProvider(language, {
      provideHover: async (model, position) => {
        const file = fileForModel(model);
        if (!file) return null;
        const result = await lspDocumentRequest<any>(
          file,
          "textDocument/hover",
          textDocumentPosition(file, position),
        );
        if (!result?.contents) return null;
        return {
          contents: hoverContents(result.contents),
          range: result.range ? toMonacoRange(result.range) : undefined,
        };
      },
    });

    monaco.languages.registerDefinitionProvider(language, {
      provideDefinition: async (model, position) => {
        const file = fileForModel(model);
        if (!file) return null;
        const result = await lspDocumentRequest<any>(
          file,
          "textDocument/definition",
          textDocumentPosition(file, position),
        );
        return toLocations(result);
      },
    });

    monaco.languages.registerReferenceProvider(language, {
      provideReferences: async (model, position, context) => {
        const file = fileForModel(model);
        if (!file) return null;
        const result = await lspDocumentRequest<any>(
          file,
          "textDocument/references",
          {
            ...textDocumentPosition(file, position),
            context: { includeDeclaration: context.includeDeclaration },
          },
        );
        return toLocations(result) as monaco.languages.Location[];
      },
    });

    monaco.languages.registerRenameProvider(language, {
      provideRenameEdits: async (model, position, newName) => {
        const file = fileForModel(model);
        if (!file) return { edits: [] };
        const result = await lspDocumentRequest<any>(
          file,
          "textDocument/rename",
          {
            ...textDocumentPosition(file, position),
            newName,
          },
        );
        return toWorkspaceEdit(result);
      },
    });

    monaco.languages.registerSignatureHelpProvider(language, {
      signatureHelpTriggerCharacters: ["(", ","],
      signatureHelpRetriggerCharacters: [")"],
      provideSignatureHelp: async (model, position) => {
        const file = fileForModel(model);
        if (!file) return null;
        const result = await lspDocumentRequest<any>(
          file,
          "textDocument/signatureHelp",
          textDocumentPosition(file, position),
        );
        if (!result?.signatures?.length) return null;
        const toDoc = (doc: any) =>
          doc
            ? {
                value:
                  typeof doc === "string" ? doc : (doc.value ?? ""),
              }
            : undefined;
        return {
          value: {
            signatures: result.signatures.map((sig: any) => ({
              label: sig.label,
              documentation: toDoc(sig.documentation),
              parameters: (sig.parameters ?? []).map((p: any) => ({
                label:
                  typeof p.label === "string"
                    ? p.label
                    : sig.label.slice(p.label[0], p.label[1]),
                documentation: toDoc(p.documentation),
              })),
            })),
            activeSignature: result.activeSignature ?? 0,
            activeParameter: result.activeParameter ?? 0,
          },
          dispose: () => {},
        };
      },
    });

    monaco.languages.registerDocumentSymbolProvider(language, {
      provideDocumentSymbols: async (model) => {
        const file = fileForModel(model);
        if (!file) return null;
        const result = await lspDocumentRequest<any>(
          file,
          "textDocument/documentSymbol",
          { textDocument: { uri: pathToUri(file.path) } },
        );
        return toDocumentSymbols(result);
      },
    });
  }
}
