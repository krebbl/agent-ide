import { monaco } from "../../utils/monacoSetup";

/* eslint-disable @typescript-eslint/no-explicit-any */

export function pathToUri(path: string): string {
  let out = "file://";
  if (!path.startsWith("/")) out += "/";
  for (const ch of path) {
    if (/[A-Za-z0-9\-._~/]/.test(ch)) {
      out += ch;
    } else {
      out += encodeURIComponent(ch);
    }
  }
  return out;
}

export function uriToPath(uri: string): string {
  const withoutScheme = uri.replace(/^file:\/\//, "");
  return decodeURIComponent(withoutScheme);
}

export function toLspPosition(position: {
  lineNumber: number;
  column: number;
}): { line: number; character: number } {
  return { line: position.lineNumber - 1, character: position.column - 1 };
}

export function toMonacoRange(range: any): InstanceType<typeof monaco.Range> {
  return new monaco.Range(
    range.start.line + 1,
    range.start.character + 1,
    range.end.line + 1,
    range.end.character + 1,
  );
}

const COMPLETION_KIND_MAP: Record<number, monaco.languages.CompletionItemKind> = {
  1: monaco.languages.CompletionItemKind.Text,
  2: monaco.languages.CompletionItemKind.Method,
  3: monaco.languages.CompletionItemKind.Function,
  4: monaco.languages.CompletionItemKind.Constructor,
  5: monaco.languages.CompletionItemKind.Field,
  6: monaco.languages.CompletionItemKind.Variable,
  7: monaco.languages.CompletionItemKind.Class,
  8: monaco.languages.CompletionItemKind.Interface,
  9: monaco.languages.CompletionItemKind.Module,
  10: monaco.languages.CompletionItemKind.Property,
  11: monaco.languages.CompletionItemKind.Unit,
  12: monaco.languages.CompletionItemKind.Value,
  13: monaco.languages.CompletionItemKind.Enum,
  14: monaco.languages.CompletionItemKind.Keyword,
  15: monaco.languages.CompletionItemKind.Snippet,
  16: monaco.languages.CompletionItemKind.Color,
  17: monaco.languages.CompletionItemKind.File,
  18: monaco.languages.CompletionItemKind.Reference,
  19: monaco.languages.CompletionItemKind.Folder,
  20: monaco.languages.CompletionItemKind.EnumMember,
  21: monaco.languages.CompletionItemKind.Constant,
  22: monaco.languages.CompletionItemKind.Struct,
  23: monaco.languages.CompletionItemKind.Event,
  24: monaco.languages.CompletionItemKind.Operator,
  25: monaco.languages.CompletionItemKind.TypeParameter,
};

function toMarkdownString(contents: any): { value: string } | undefined {
  if (!contents) return undefined;
  if (typeof contents === "string") return { value: contents };
  if (contents.kind === "markdown" || contents.kind === "plaintext") {
    return { value: contents.value };
  }
  if (contents.language) {
    return { value: "```" + contents.language + "\n" + contents.value + "\n```" };
  }
  if (contents.value) return { value: contents.value };
  return undefined;
}

export function hoverContents(contents: any): { value: string }[] {
  const list = Array.isArray(contents) ? contents : [contents];
  return list
    .map(toMarkdownString)
    .filter((c): c is { value: string } => c !== undefined);
}

export function toCompletionItem(
  item: any,
  fallbackRange: InstanceType<typeof monaco.Range>,
): monaco.languages.CompletionItem {
  const textEdit = item.textEdit;
  return {
    label: item.label,
    kind:
      COMPLETION_KIND_MAP[item.kind] ??
      monaco.languages.CompletionItemKind.Text,
    insertText: item.insertText ?? textEdit?.newText ?? item.label,
    range: textEdit?.range ? toMonacoRange(textEdit.range) : fallbackRange,
    detail: item.detail,
    documentation: toMarkdownString(item.documentation),
    sortText: item.sortText,
    filterText: item.filterText,
  };
}

export function toMonacoUri(lspUri: string): InstanceType<typeof monaco.Uri> {
  return monaco.Uri.parse(uriToPath(lspUri));
}

export function toLocations(
  result: any,
): (monaco.languages.Location | monaco.languages.LocationLink)[] {
  if (!result) return [];
  const list = Array.isArray(result) ? result : [result];
  return list.map((loc: any) => {
    if (loc.targetUri) {
      return {
        uri: toMonacoUri(loc.targetUri),
        range: toMonacoRange(loc.targetSelectionRange ?? loc.targetRange),
        originSelectionRange: loc.originSelectionRange
          ? toMonacoRange(loc.originSelectionRange)
          : undefined,
        targetSelectionRange: loc.targetSelectionRange
          ? toMonacoRange(loc.targetSelectionRange)
          : undefined,
      } as monaco.languages.LocationLink;
    }
    return {
      uri: toMonacoUri(loc.uri),
      range: toMonacoRange(loc.range),
    } as monaco.languages.Location;
  });
}

export function toWorkspaceEdit(result: any): monaco.languages.WorkspaceEdit {
  const edits: monaco.languages.IWorkspaceTextEdit[] = [];
  if (result?.changes) {
    for (const [uri, textEdits] of Object.entries<any[]>(result.changes)) {
      for (const edit of textEdits) {
        edits.push({
          resource: toMonacoUri(uri),
          versionId: undefined,
          textEdit: { range: toMonacoRange(edit.range), text: edit.newText },
        });
      }
    }
  }
  if (Array.isArray(result?.documentChanges)) {
    for (const docChange of result.documentChanges) {
      if (!docChange.textDocument || !Array.isArray(docChange.edits)) continue;
      for (const edit of docChange.edits) {
        edits.push({
          resource: toMonacoUri(docChange.textDocument.uri),
          versionId: undefined,
          textEdit: { range: toMonacoRange(edit.range), text: edit.newText },
        });
      }
    }
  }
  return { edits };
}

export function toDocumentSymbols(result: any): monaco.languages.DocumentSymbol[] {
  if (!Array.isArray(result)) return [];
  return result.map((sym: any) => {
    if (sym.selectionRange) {
      return {
        name: sym.name,
        detail: sym.detail ?? "",
        kind: sym.kind - 1,
        range: toMonacoRange(sym.range),
        selectionRange: toMonacoRange(sym.selectionRange),
        tags: sym.tags ?? [],
        children: sym.children ? toDocumentSymbols(sym.children) : [],
      };
    }
    const range = toMonacoRange(sym.location.range);
    return {
      name: sym.containerName ? `${sym.containerName}.${sym.name}` : sym.name,
      detail: "",
      kind: sym.kind - 1,
      range,
      selectionRange: range,
      tags: [],
      children: [],
    };
  });
}

const SEVERITY_MAP: Record<number, monaco.MarkerSeverity> = {
  1: monaco.MarkerSeverity.Error,
  2: monaco.MarkerSeverity.Warning,
  3: monaco.MarkerSeverity.Info,
  4: monaco.MarkerSeverity.Hint,
};

export function toMarkers(diagnostics: any[]): monaco.editor.IMarkerData[] {
  return diagnostics.map((d) => ({
    severity: SEVERITY_MAP[d.severity ?? 1] ?? monaco.MarkerSeverity.Error,
    message: d.message,
    source: d.source,
    code: d.code != null ? String(d.code) : undefined,
    startLineNumber: d.range.start.line + 1,
    startColumn: d.range.start.character + 1,
    endLineNumber: d.range.end.line + 1,
    endColumn: d.range.end.character + 1,
  }));
}
