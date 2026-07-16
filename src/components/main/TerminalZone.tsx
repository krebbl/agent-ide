import { Terminal, Plus } from "lucide-react";

export default function TerminalZone() {
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-[var(--color-surface0)] px-3">
        <Terminal size={14} className="text-[var(--color-green)]" />
        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
          Terminal
        </span>
        <button className="ml-auto text-[var(--color-overlay1)] transition-colors hover:text-[var(--color-blue)]">
          <Plus size={16} />
        </button>
      </div>
      <div className="flex flex-1 items-center justify-center p-4">
        <span className="text-sm text-[var(--color-overlay0)]">
          No terminal sessions
        </span>
      </div>
    </div>
  );
}
