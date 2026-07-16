export default function RightSidebar() {
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center border-b border-[var(--color-surface0)] px-3">
        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
          Files
        </span>
      </div>
      <div className="flex flex-1 items-center justify-center p-4">
        <span className="text-sm text-[var(--color-overlay0)]">
          No file tree
        </span>
      </div>
    </div>
  );
}
