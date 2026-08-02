import { FileWarning } from "lucide-react";
import Dialog from "../ui/Dialog";

interface ConfirmCloseDialogProps {
  name: string;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}

export default function ConfirmCloseDialog({
  name,
  onSave,
  onDiscard,
  onCancel,
}: ConfirmCloseDialogProps) {
  return (
    <Dialog
      title="Unsaved Changes"
      icon={<FileWarning size={16} className="text-[var(--color-peach)]" />}
      width="420px"
      onClose={onCancel}
      footer={
        <>
          <button
            onClick={onCancel}
            className="rounded-md px-4 py-2 text-sm text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]"
          >
            Cancel
          </button>
          <button
            onClick={onDiscard}
            className="rounded-md px-4 py-2 text-sm text-[var(--color-red)] hover:bg-[var(--color-surface0)]"
          >
            Don't Save
          </button>
          <button
            onClick={onSave}
            className="rounded-md bg-[var(--color-blue)] px-4 py-2 text-sm font-medium text-[var(--color-crust)] hover:bg-[var(--color-blue)]/80"
          >
            Save
          </button>
        </>
      }
    >
      <p className="text-sm text-[var(--color-text)]">
        <span className="font-medium">"{name}"</span> has unsaved changes. Do
        you want to save them before closing?
      </p>
    </Dialog>
  );
}
