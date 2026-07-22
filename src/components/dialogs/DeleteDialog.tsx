import { Trash2, AlertTriangle } from "lucide-react";
import Dialog from "../ui/Dialog";

interface DeleteDialogProps {
  name: string;
  isDir: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function DeleteDialog({ name, isDir, onConfirm, onCancel }: DeleteDialogProps) {
  return (
    <Dialog
      title="Delete"
      icon={<Trash2 size={16} className="text-[var(--color-red)]" />}
      width="420px"
      danger
      onClose={onCancel}
      footer={
        <>
          <button onClick={onCancel} className="rounded-md px-4 py-2 text-sm text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]">
            Cancel
          </button>
          <button
            onClick={onConfirm}
            className="rounded-md bg-[var(--color-red)] px-4 py-2 text-sm font-medium text-[var(--color-crust)] hover:bg-[var(--color-red)]/80"
          >
            Delete
          </button>
        </>
      }
    >
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
    </Dialog>
  );
}
