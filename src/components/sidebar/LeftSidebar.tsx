import { FolderPlus } from "lucide-react";

export default function LeftSidebar() {
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center justify-between border-b border-[var(--color-surface0)] px-3">
        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
          Projects
        </span>
        <button className="text-[var(--color-overlay1)] transition-colors hover:text-[var(--color-blue)]">
          <FolderPlus size={16} />
        </button>
      </div>
      <div className="flex flex-1 items-center justify-center p-4">
        <span className="text-sm text-[var(--color-overlay0)]">
          No projects yet
        </span>
      </div>
    </div>
  );
}
