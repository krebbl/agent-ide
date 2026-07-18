import { useState, useEffect, useRef } from "react";
import { X, FilePlus, FolderPlus } from "lucide-react";

interface NewItemDialogProps {
  itemType: "file" | "folder";
  parentPath: string;
  onConfirm: (name: string) => void;
  onCancel: () => void;
}

export default function NewItemDialog({ itemType, parentPath, onConfirm, onCancel }: NewItemDialogProps) {
  const [name, setName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleSubmit = () => {
    if (name.trim()) onConfirm(name.trim());
  };

  const icon = itemType === "folder" ? <FolderPlus size={16} className="text-[var(--color-blue)]" /> : <FilePlus size={16} className="text-[var(--color-blue)]" />;
  const label = itemType === "folder" ? "New Folder" : "New File";
  const placeholder = itemType === "folder" ? "folder-name" : "file-name.ext";

  const displayName = parentPath.split("/").filter(Boolean).pop() ?? parentPath;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onCancel}>
      <div className="flex w-[400px] flex-col rounded-lg border border-[var(--color-surface0)] bg-[var(--color-mantle)] shadow-2xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 border-b border-[var(--color-surface0)] px-4 py-3">
          {icon}
          <span className="text-sm font-semibold text-[var(--color-text)]">{label}</span>
          <button onClick={onCancel} className="ml-auto rounded p-0.5 text-[var(--color-overlay1)] hover:text-[var(--color-text)] hover:bg-[var(--color-surface0)]">
            <X size={16} />
          </button>
        </div>

        <div className="p-4">
          <div className="mb-1 text-xs text-[var(--color-overlay0)] truncate" title={parentPath}>
            In: {displayName}
          </div>
          <input
            ref={inputRef}
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleSubmit(); if (e.key === "Escape") onCancel(); }}
            placeholder={placeholder}
            className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder:[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
          />
        </div>

        <div className="flex justify-end gap-2 border-t border-[var(--color-surface0)] px-4 py-3">
          <button onClick={onCancel} className="rounded-md px-4 py-2 text-sm text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]">
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={!name.trim()}
            className="rounded-md bg-[var(--color-blue)] px-4 py-2 text-sm font-medium text-[var(--color-crust)] hover:bg-[var(--color-blue)]/80 disabled:opacity-50"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
