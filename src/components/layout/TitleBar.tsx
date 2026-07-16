export default function TitleBar() {
  return (
    <div
      className="flex h-9 shrink-0 items-center border-b border-[var(--color-surface0)] bg-[var(--color-crust)] px-3"
      data-tauri-drag-region
    >
      <span className="text-sm font-semibold text-[var(--color-text)]">
        Agent IDE
      </span>
    </div>
  );
}
