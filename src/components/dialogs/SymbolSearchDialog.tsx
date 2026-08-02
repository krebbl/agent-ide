import { useEffect, useState } from "react";
import { Search } from "lucide-react";
import Dialog from "../ui/Dialog";
import { useEditorStore, type OpenFile } from "../../stores/editorStore";
import { useFileTreeStore } from "../../stores/fileTreeStore";
import { lspDocumentRequest } from "../../services/lsp/coordinator";
import { uriToPath } from "../../services/lsp/converters";

/* eslint-disable @typescript-eslint/no-explicit-any */

const SYMBOL_KINDS = [
  "File", "Module", "Namespace", "Package", "Class", "Method", "Property",
  "Field", "Constructor", "Enum", "Interface", "Function", "Variable",
  "Constant", "String", "Number", "Boolean", "Array", "Object", "Key",
  "Null", "EnumMember", "Struct", "Event", "Operator", "TypeParameter",
];

interface SymbolResult {
  name: string;
  kind: number;
  location: { uri: string; range: any };
  containerName?: string;
}

interface SymbolSearchDialogProps {
  file: OpenFile;
  onClose: () => void;
}

export default function SymbolSearchDialog({
  file,
  onClose,
}: SymbolSearchDialogProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SymbolResult[]>([]);
  const [selected, setSelected] = useState(0);

  useEffect(() => {
    const handle = setTimeout(async () => {
      const result = await lspDocumentRequest<SymbolResult[]>(
        file,
        "workspace/symbol",
        { query },
      );
      setResults(result ?? []);
      setSelected(0);
    }, 200);
    return () => clearTimeout(handle);
  }, [query, file]);

  const openSymbol = (sym: SymbolResult) => {
    const path = uriToPath(sym.location.uri);
    const { openFiles, openFile, setActive, setPendingReveal } =
      useEditorStore.getState();
    const projectId =
      openFiles.find((f) => f.path === path)?.projectId ?? file.projectId;
    setPendingReveal({
      path,
      line: sym.location.range.start.line + 1,
      column: sym.location.range.start.character + 1,
    });
    if (openFiles.some((f) => f.path === path)) {
      setActive(path);
    } else {
      void openFile(projectId, path);
    }
    onClose();
  };

  const rootPath = useFileTreeStore.getState().rootPath;
  const displayPath = (path: string) =>
    rootPath && path.startsWith(rootPath + "/")
      ? path.slice(rootPath.length + 1)
      : path;

  return (
    <Dialog
      title="Go to Symbol"
      icon={<Search size={16} className="text-[var(--color-blue)]" />}
      width="560px"
      onClose={onClose}
    >
      <input
        autoFocus
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setSelected((s) => Math.min(s + 1, results.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setSelected((s) => Math.max(s - 1, 0));
          } else if (e.key === "Enter" && results[selected]) {
            openSymbol(results[selected]);
          } else if (e.key === "Escape") {
            onClose();
          }
        }}
        placeholder="Search workspace symbols…"
        className="mb-2 w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-surface0)] px-3 py-2 text-sm text-[var(--color-text)] outline-none placeholder:text-[var(--color-overlay0)] focus:border-[var(--color-blue)]"
      />
      <div className="no-scrollbar max-h-80 overflow-y-auto">
        {results.length === 0 && (
          <p className="px-1 py-3 text-center text-xs text-[var(--color-overlay0)]">
            {query ? "No matching symbols" : "Type to search symbols"}
          </p>
        )}
        {results.map((sym, i) => (
          <div
            key={`${sym.location.uri}:${sym.name}:${i}`}
            className={`flex cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-xs ${
              i === selected
                ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
                : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50"
            }`}
            onClick={() => openSymbol(sym)}
            onMouseEnter={() => setSelected(i)}
          >
            <span className="shrink-0 rounded bg-[var(--color-surface1)] px-1.5 py-0.5 text-[10px] text-[var(--color-mauve)]">
              {SYMBOL_KINDS[sym.kind - 1] ?? "Symbol"}
            </span>
            <span className="truncate text-[var(--color-text)]">
              {sym.containerName ? `${sym.containerName}.` : ""}
              {sym.name}
            </span>
            <span className="ml-auto shrink-0 text-[var(--color-overlay0)]">
              {displayPath(uriToPath(sym.location.uri))}
            </span>
          </div>
        ))}
      </div>
    </Dialog>
  );
}
