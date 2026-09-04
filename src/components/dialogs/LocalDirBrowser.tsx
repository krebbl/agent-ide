import { useState, useEffect } from "react";
import { invoke } from "../../services/ipc";
import { Folder, File, ChevronRight, ArrowUp, Loader2, AlertCircle } from "lucide-react";

interface LocalDirEntry {
  name: string;
  isDir: boolean;
}

interface LocalDirBrowserProps {
  mode: "dir" | "file";
  onSelect: (path: string) => void;
  onCancel: () => void;
}

export default function LocalDirBrowser({ mode, onSelect, onCancel }: LocalDirBrowserProps) {
  const [currentPath, setCurrentPath] = useState("/");
  const [entries, setEntries] = useState<LocalDirEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    invoke<LocalDirEntry[]>("list_local_dir", { path: currentPath })
      .then((result) => {
        if (!cancelled) setEntries(mode === "dir" ? result.filter((e) => e.isDir) : result);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [currentPath, mode]);

  const handleNavigate = (name: string) => {
    setCurrentPath(currentPath.endsWith("/") ? `${currentPath}${name}` : `${currentPath}/${name}`);
  };

  const handleNavigateUp = () => {
    const parts = currentPath.split("/").filter(Boolean);
    parts.pop();
    const newPath = parts.length > 0 ? `/${parts.join("/")}` : "/";
    setCurrentPath(newPath);
  };

  const breadcrumbs = currentPath.split("/").filter(Boolean);

  return (
    <div className="mt-3 rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] p-3">
      <div className="mb-2 flex items-center gap-1 overflow-x-auto text-xs">
        <button
          onClick={() => setCurrentPath("/")}
          className="rounded px-1.5 py-0.5 text-[var(--color-blue)] hover:bg-[var(--color-surface0)]"
        >
          /
        </button>
        {breadcrumbs.map((part, i) => (
          <span key={i} className="flex items-center gap-1">
            <ChevronRight size={10} className="text-[var(--color-overlay0)]" />
            <button
              onClick={() => setCurrentPath(`/${breadcrumbs.slice(0, i + 1).join("/")}`)}
              className="rounded px-1.5 py-0.5 text-[var(--color-blue)] hover:bg-[var(--color-surface0)]"
            >
              {part}
            </button>
          </span>
        ))}
      </div>

      {currentPath !== "/" && (
        <button
          onClick={handleNavigateUp}
          className="mb-2 flex w-full items-center gap-2 rounded px-2 py-1.5 text-sm text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]"
        >
          <ArrowUp size={14} />
          ..
        </button>
      )}

      {loading && (
        <div className="flex items-center gap-2 py-4 text-sm text-[var(--color-overlay1)]">
          <Loader2 size={14} className="animate-spin" />
          Loading...
        </div>
      )}

      {error && (
        <div className="flex items-center gap-2 py-2 text-sm text-[var(--color-red)]">
          <AlertCircle size={14} />
          {error}
        </div>
      )}

      {!loading && !error && (
        <div className="max-h-48 space-y-0.5 overflow-y-auto">
          {entries.length === 0 && (
            <div className="py-2 text-center text-sm text-[var(--color-overlay0)]">
              {mode === "dir" ? "No directories" : "No entries"}
            </div>
          )}
          {entries.map((entry) => (
            <button
              key={entry.name}
              onClick={() => (entry.isDir ? handleNavigate(entry.name) : onSelect(`${currentPath}/${entry.name}`))}
              className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface0)]"
            >
              {entry.isDir ? (
                <Folder size={14} className="text-[var(--color-blue)]" />
              ) : (
                <File size={14} className="text-[var(--color-overlay1)]" />
              )}
              {entry.name}
            </button>
          ))}
        </div>
      )}

      {mode === "dir" && (
        <div className="mt-3 flex items-center justify-end border-t border-[var(--color-surface0)] pt-2">
          <div className="flex gap-2">
            <button
              onClick={onCancel}
              className="rounded px-3 py-1.5 text-xs text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]"
            >
              Cancel
            </button>
            <button
              onClick={() => onSelect(currentPath)}
              className="rounded bg-[var(--color-blue)] px-3 py-1.5 text-xs font-medium text-[var(--color-crust)] hover:bg-[var(--color-blue)]/80"
            >
              Select This Folder
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
