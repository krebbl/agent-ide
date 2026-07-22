import { useState, useEffect, useRef } from "react";
import { Pencil } from "lucide-react";
import Dialog from "../ui/Dialog";

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
    <Dialog
      title="Rename"
      icon={<Pencil size={16} className="text-[var(--color-yellow)]" />}
      onClose={onCancel}
      footer={
        <>
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
        </>
      }
    >
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
    </Dialog>
  );
}
