import { useRef, useEffect, useLayoutEffect, useState } from "react";
import { FilePlus, FolderPlus, Pencil, Trash2, Copy, FolderOpen, X } from "lucide-react";

interface FileTreeContextMenuProps {
  x: number;
  y: number;
  entryPath: string;
  name: string;
  isDir: boolean;
  onClose: () => void;
  onNewFile: (parent: string) => void;
  onNewFolder: (parent: string) => void;
  onRename: (entryPath: string, isDir: boolean, name: string) => void;
  onDelete: (entryPath: string, isDir: boolean, name: string) => void;
  onCopyPath: (path: string) => Promise<void>;
  onCopyRelativePath: () => Promise<void>;
}

export default function FileTreeContextMenu({
  x, y, entryPath, name, isDir,
  onClose, onNewFile, onNewFolder, onRename, onDelete, onCopyPath, onCopyRelativePath,
}: FileTreeContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });

  // Clamp position so the menu stays inside the viewport
  useLayoutEffect(() => {
    if (!menuRef.current) return;
    const rect = menuRef.current.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const gap = 8;
    const adjustedX = x + rect.width > vw ? vw - rect.width - gap : x;
    const adjustedY = y + rect.height > vh ? vh - rect.height - gap : y;
    setPos({
      x: Math.max(gap, adjustedX),
      y: Math.max(gap, adjustedY),
    });
  }, [x, y]);

  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) onClose();
    };
    const handleKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
    };
  }, [onClose]);

  const parentOf = (p: string) => p.substring(0, p.lastIndexOf("/"));

  const handleNewFile = () => {
    const parent = isDir ? entryPath : parentOf(entryPath);
    onClose();
    onNewFile(parent);
  };

  const handleNewFolder = () => {
    const parent = isDir ? entryPath : parentOf(entryPath);
    onClose();
    onNewFolder(parent);
  };

  const handleRename = () => {
    onClose();
    onRename(entryPath, isDir, name);
  };

  const handleDelete = () => {
    onClose();
    onDelete(entryPath, isDir, name);
  };

  const handleCopyPath = () => {
    onCopyPath(entryPath);
  };

  const handleCopyRelativePath = () => {
    onCopyRelativePath();
  };

  return (
    <div
      ref={menuRef}
      className="fixed z-50 min-w-[190px] rounded-md border border-[var(--color-surface0)] bg-[var(--color-mantle)] py-1 shadow-2xl"
      style={{ left: pos.x, top: pos.y }}
    >
      {/* Header */}
      <div className="flex items-center gap-1.5 border-b border-[var(--color-surface0)] px-3 py-1.5">
        <FolderOpen size={12} className="text-[var(--color-overlay0)]" />
        <span className="truncate text-xs text-[var(--color-overlay0)]">{name}</span>
        <button onClick={onClose} className="ml-auto rounded p-0.5 text-[var(--color-overlay0)] hover:text-[var(--color-text)] hover:bg-[var(--color-surface0)]">
          <X size={11} />
        </button>
      </div>

      {/* Create actions */}
      <button onClick={handleNewFile} className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface0)]">
        <FilePlus size={14} className="text-[var(--color-blue)]" />
        New File
      </button>
      <button onClick={handleNewFolder} className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface0)]">
        <FolderPlus size={14} className="text-[var(--color-blue)]" />
        New Folder
      </button>

      <div className="my-1 border-t border-[var(--color-surface0)]" />

      {/* Mutating */}
      <button onClick={handleRename} className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface0)]">
        <Pencil size={14} className="text-[var(--color-yellow)]" />
        Rename
      </button>
      <button onClick={handleDelete} className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-[var(--color-red)] hover:bg-[var(--color-surface0)]">
        <Trash2 size={14} />
        Delete
      </button>

      <div className="my-1 border-t border-[var(--color-surface0)]" />

      {/* Info */}
      <button onClick={handleCopyPath} className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface0)]">
        <Copy size={14} className="text-[var(--color-overlay1)]" />
        Copy Path
      </button>
      <button onClick={handleCopyRelativePath} className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface0)]">
        <Copy size={14} className="text-[var(--color-overlay1)]" />
        Copy Relative Path
      </button>
    </div>
  );
}
