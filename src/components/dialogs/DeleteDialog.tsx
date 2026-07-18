import { X, Trash2, AlertTriangle } from "lucide-react";

interface DeleteDialogProps {
  name: string;
  isDir: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function DeleteDialog({ name, isDir, onConfirm, onCancel }: DeleteDialogProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onCancel}>
      <div className="flex w-[420px] flex-col rounded-lg border border-[var(--color-red)]/30 bg-[var(--color-mantle)] shadow-2xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 border-b border-[var(--color-surface0)] px-4 py-3">
          <Trash2 size={16} className="text-[var(--color-red)]" />
          <span className="text-sm font-semibold text-[var(--color-red)]">Delete</span>
          <button onClick={onCancel} className="ml-auto rounded p-0.5 text-[var(--color-overlay1)] hover:text-[var(--color-text)] hover:bg-[var(--color-surface0)]">
            <X size={16} />
          </button>
        </div>

        <div className="p-4">
          <div className="flex items-start gap-3 rounded-md border border-[var(--color-peach)]/30 bg-[var(--color-peach)]/10 p-3">
            <AlertTriangle size={18} className="mt-0.5 shrink-0 text-[var(--color-peach)]" />
            <div>
              <p className="text-sm text-[var(--color-text)]">
                Are you sure you want to delete <span className="font-medium">"{name}"</span>?
              </p>
              {isDir && (
                <p className="mt-1 text-xs text-[var(--color-peach)]">
                  This folder and all its contents will be permanently removed.
                </p>
              )}
            </div>
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-[var(--color-surface0)] px-4 py-3">
          <button onClick={onCancel} className="rounded-md px-4 py-2 text-sm text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]">
            Cancel
          </button>
          <button
            onClick={onConfirm}
            className="rounded-md bg-[var(--color-red)] px-4 py-2 text-sm font-medium text-[var(--color-crust)] hover:bg-[var(--color-red)]/80"
          >
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}
