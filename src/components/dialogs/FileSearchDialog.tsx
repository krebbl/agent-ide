import { useEffect, useRef, useState } from "react";
import { Search } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import Dialog from "../ui/Dialog";
import { useEditorStore } from "../../stores/editorStore";
import { useFileTreeStore } from "../../stores/fileTreeStore";

interface FileSearchDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function FileSearchDialog({
  isOpen,
  onClose,
}: FileSearchDialogProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<string[]>([]);
  const [selected, setSelected] = useState(0);
  const [loading, setLoading] = useState(false);

  const inputRef = useRef<HTMLInputElement>(null);

  const rootPath = useFileTreeStore((s) => s.rootPath);
  const projectId = useFileTreeStore((s) => s.projectId);

  // Reset state when dialog opens
  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setResults([]);
      setSelected(0);
      setLoading(false);
    }
  }, [isOpen]);

  // Focus input when dialog opens
  useEffect(() => {
    if (isOpen) {
      // Small delay to ensure DOM is ready
      const handle = requestAnimationFrame(() => inputRef.current?.focus());
      return () => cancelAnimationFrame(handle);
    }
  }, [isOpen]);

  // Debounced search
  useEffect(() => {
    if (!query.trim() || !rootPath || !projectId) {
      setResults([]);
      setLoading(false);
      return;
    }

    setLoading(true);
    const handle = setTimeout(async () => {
      try {
        const paths = await invoke<string[]>("fs_search_files", {
          projectId,
          root: rootPath,
          query,
          limit: 100,
        });
        setResults(paths);
        setSelected(0);
      } catch {
        setResults([]);
      } finally {
        setLoading(false);
      }
    }, 150);

    return () => clearTimeout(handle);
  }, [query, rootPath, projectId]);

  const openFile = (fullPath: string) => {
    if (!projectId) return;
    useEditorStore.getState().openFile(projectId, fullPath);
    onClose();
  };

  if (!isOpen || !rootPath || !projectId) {
    return null;
  }

  const displayPath = (path: string) =>
    path.startsWith(rootPath + "/") ? path.slice(rootPath.length + 1) : path;

  const highlightMatch = (text: string, q: string) => {
    if (!q.trim()) return text;
    const terms = q.split(/\s+/).filter(Boolean);
    if (terms.length === 0) return text;

    const lower = text.toLowerCase();
    const intervals: Array<[number, number]> = [];

    for (const term of terms) {
      const termLower = term.toLowerCase();
      let start = 0;
      while (true) {
        const idx = lower.indexOf(termLower, start);
        if (idx === -1) break;
        intervals.push([idx, idx + termLower.length]);
        start = idx + termLower.length;
      }
    }

    if (intervals.length === 0) return text;

    intervals.sort((a, b) => a[0] - b[0]);
    const merged: Array<[number, number]> = [];
    for (const [start, end] of intervals) {
      const last = merged[merged.length - 1];
      if (!last || last[1] < start) {
        merged.push([start, end]);
      } else {
        last[1] = Math.max(last[1], end);
      }
    }

    const parts: React.ReactNode[] = [];
    let lastEnd = 0;
    for (const [start, end] of merged) {
      if (start > lastEnd) parts.push(text.slice(lastEnd, start));
      parts.push(
        <strong key={start} className="text-[var(--color-blue)]">
          {text.slice(start, end)}
        </strong>
      );
      lastEnd = end;
    }
    if (lastEnd < text.length) parts.push(text.slice(lastEnd));

    return <>{parts}</>;
  };

  return (
    <Dialog
      title="Search Files"
      icon={<Search size={16} className="text-[var(--color-blue)]" />}
      width="560px"
      placement="top"
      onClose={onClose}
    >
      <input
        ref={inputRef}
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
            openFile(results[selected]);
          } else if (e.key === "Escape") {
            onClose();
          }
        }}
        placeholder="Search files by name…"
        className="mb-2 w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-surface0)] px-3 py-2 text-sm text-[var(--color-text)] outline-none placeholder:text-[var(--color-overlay0)] focus:border-[var(--color-blue)]"
      />
      <div className="no-scrollbar max-h-80 overflow-y-auto">
        {loading && (
          <p className="px-1 py-3 text-center text-xs text-[var(--color-overlay0)]">
            Searching…
          </p>
        )}
        {!loading && results.length === 0 && (
          <p className="px-1 py-3 text-center text-xs text-[var(--color-overlay0)]">
            {query ? "No matching files" : "Type to search files"}
          </p>
        )}
        {results.map((path, i) => (
          <div
            key={path}
            className={`flex cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-xs ${
              i === selected
                ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
                : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50"
            }`}
            onClick={() => openFile(path)}
            onMouseEnter={() => setSelected(i)}
          >
            <span className="truncate text-[var(--color-text)]">
              {highlightMatch(displayPath(path), query)}
            </span>
          </div>
        ))}
      </div>
    </Dialog>
  );
}