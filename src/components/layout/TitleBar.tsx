import { useProjectStore } from "../../stores/projectStore";
import { Folder, Server } from "lucide-react";

export default function TitleBar() {
  const { getActiveProject } = useProjectStore();
  const activeProject = getActiveProject();

  return (
    <div
      className="flex h-9 shrink-0 items-center border-b border-[var(--color-surface0)] bg-[var(--color-crust)] px-3"
      data-tauri-drag-region
    >
      <span className="text-sm font-semibold text-[var(--color-text)]">
        Agent IDE
      </span>
      {activeProject && (
        <>
          <span className="mx-2 text-[var(--color-surface2)]">/</span>
          <span className="flex items-center gap-1.5 text-sm text-[var(--color-subtext1)]">
            {activeProject.type === "local" ? (
              <Folder size={13} className="text-[var(--color-blue)]" />
            ) : (
              <Server size={13} className="text-[var(--color-mauve)]" />
            )}
            {activeProject.name}
          </span>
        </>
      )}
    </div>
  );
}
