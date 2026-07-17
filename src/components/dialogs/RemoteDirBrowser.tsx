import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Folder, ChevronRight, ArrowUp, Check, Loader2, AlertCircle } from "lucide-react";

interface SshDirEntry {
  name: string;
  is_dir: boolean;
}

interface RemoteDirBrowserProps {
  currentPath: string;
  onNavigate: (path: string) => void;
  onSelect: (path: string) => void;
  onCancel: () => void;
}

export default function RemoteDirBrowser({
  currentPath,
  onNavigate,
  onSelect,
  onCancel,
}: RemoteDirBrowserProps) {
  const [entries, setEntries] = useState<SshDirEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hasGit, setHasGit] = useState<boolean | null>(null);

  const loadDirectory = async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<SshDirEntry[]>("ssh_list_directory", {
        projectId: "temp",
        path,
      });
      setEntries(result.filter((e) => e.is_dir).sort((a, b) => a.name.localeCompare(b.name)));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadDirectory(currentPath);
  }, [currentPath]);

  const handleNavigate = (dirName: string) => {
    const newPath = currentPath.endsWith("/")
      ? `${currentPath}${dirName}`
      : `${currentPath}/${dirName}`;
    onNavigate(newPath);
  };

  const handleNavigateUp = () => {
    const parts = currentPath.split("/").filter(Boolean);
    parts.pop();
    const newPath = parts.length > 0 ? `/${parts.join("/")}` : "/";
    onNavigate(newPath);
  };

  const handleSelect = async () => {
    try {
      const result = await invoke<boolean>("ssh_check_git", {
        projectId: "temp",
        path: currentPath,
      });
      setHasGit(result);
      if (result) {
        onSelect(currentPath);
      }
    } catch {
      setHasGit(false);
    }
  };

  const breadcrumbs = currentPath.split("/").filter(Boolean);

  return (
    <div className="mt-3 rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] p-3">
      <div className="mb-2 flex items-center gap-1 overflow-x-auto text-xs">
        <button
          onClick={() => onNavigate("/")}
          className="rounded px-1.5 py-0.5 text-[var(--color-blue)] hover:bg-[var(--color-surface0)]"
        >
          /
        </button>
        {breadcrumbs.map((part, i) => (
          <span key={i} className="flex items-center gap-1">
            <ChevronRight size={10} className="text-[var(--color-overlay0)]" />
            <button
              onClick={() => {
                const path = `/${breadcrumbs.slice(0, i + 1).join("/")}`;
                onNavigate(path);
              }}
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
            <div className="py-2 text-center text-sm text-[var(--color-overlay0)]">No directories</div>
          )}
          {entries.map((entry) => (
            <button
              key={entry.name}
              onClick={() => handleNavigate(entry.name)}
              className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface0)]"
            >
              <Folder size={14} className="text-[var(--color-blue)]" />
              {entry.name}
            </button>
          ))}
        </div>
      )}

      <div className="mt-3 flex items-center justify-between border-t border-[var(--color-surface0)] pt-2">
        {hasGit === false && (
          <span className="flex items-center gap-1 text-xs text-[var(--color-peach)]">
            <AlertCircle size={12} />
            No .git directory found
          </span>
        )}
        {hasGit === true && (
          <span className="flex items-center gap-1 text-xs text-[var(--color-green)]">
            <Check size={12} />
            Git repository found
          </span>
        )}
        <div className="ml-auto flex gap-2">
          <button
            onClick={onCancel}
            className="rounded px-3 py-1.5 text-xs text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]"
          >
            Cancel
          </button>
          <button
            onClick={handleSelect}
            className="rounded bg-[var(--color-blue)] px-3 py-1.5 text-xs font-medium text-[var(--color-crust)] hover:bg-[var(--color-blue)]/80"
          >
            Select This Folder
          </button>
        </div>
      </div>
    </div>
  );
}
