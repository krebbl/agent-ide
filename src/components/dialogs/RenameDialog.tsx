import { useState, useEffect, useRef } from "react";
import { X, Pencil } from "lucide-react";

interface RenameDialogProps {
  currentName: string;
  isDir: boolean;
  onConfirm: (newName: string) => void;
  onCancel: () => void;
}

export default function RenameDialog({ currentName, isDir, onConfirm, onCancel }: RenameDialogProps) {
  const [name, setName] = useState(currentName);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const el = inputRef.current;
    if (!el) return;
    el.focus();
    const dotIdx = currentName.lastIndexOf(".");
    if (dotIdx > 0 && !isDir) {
      el.setSelectionRange(0, dotIdx);
    } else {
      el.select();
    }
  }, [currentName, isDir]);

  const handleSubmit = () => {
    if (name.trim() && name.trim() !== currentName) {
      onConfirm(name.trim());
    } else {
      onCancel();
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onCancel}>
      <div className="flex w-[400px] flex-col rounded-lg border border-[var(--color-surface0)] bg-[var(--color-mantle)] shadow-2xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 border-b border-[var(--color-surface0)] px-4 py-3">
          <Pencil size={16} className="text-[var(--color-yellow)]" />
          <span className="text-sm font-semibold text-[var(--color-text)]">Rename</span>
          <button onClick={onCancel} className="ml-auto rounded p-0.5 text-[var(--color-overlay1)] hover:text-[var(--color-text)] hover:bg-[var(--color-surface0)]">
            <X size={16} />
          </button>
        </div>

        <div className="p-4">
          <div className="mb-2 text-xs text-[var(--color-overlay0)]">
            {isDir ? "Folder" : "File"}: <span className="text-[var(--color-text)]">{currentName}</span>
          </div>
          <input
            ref={inputRef}
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleSubmit(); if (e.key === "Escape") onCancel(); }}
            className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] focus:border-[var(--color-blue)] focus:outline-none"
          />
        </div>

        <div className="flex justify-end gap-2 border-t border-[var(--color-surface0)] px-4 py-3">
          <button onClick={onCancel} className="rounded-md px-4 py-2 text-sm text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]">
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={!name.trim() || name.trim() === currentName}
            className="rounded-md bg-[var(--color-blue)] px-4 py-2 text-sm font-medium text-[var(--color-crust)] hover:bg-[var(--color-blue)]/80 disabled:opacity-50"
          >
            Rename
          </button>
        </div>
      </div>
    </div>
  );
}
