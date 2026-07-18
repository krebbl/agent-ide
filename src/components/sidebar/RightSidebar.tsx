import { useEffect, useRef } from "react";
import { useFileTreeStore } from "../../stores/fileTreeStore";
import { useProjectStore } from "../../stores/projectStore";
import FileTree from "./FileTree";

export default function RightSidebar() {
  const { setRoot } = useFileTreeStore();
  const { projects, activeProjectId } = useProjectStore();
  const lastKey = useRef("");

  useEffect(() => {
    // Use activeProjectId to select the correct project
    const activeProject = activeProjectId
      ? projects.find((p) => p.id === activeProjectId)
      : projects.find((p) => p.worktrees.length > 0);
    if (!activeProject) return;

    const worktree =
      activeProject.activeWorktreeId
        ? activeProject.worktrees.find((w) => w.id === activeProject.activeWorktreeId)
        : activeProject.worktrees.find((w) => w.isMain);
    if (!worktree || !worktree.path) return;

    const key = `${activeProject.id}:${activeProject.type}:${worktree.path}`;
    if (key === lastKey.current) return;
    lastKey.current = key;

    setRoot(worktree.path, activeProject.id, activeProject.type);
  }, [activeProjectId, projects, setRoot]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center border-b border-[var(--color-surface0)] px-3">
        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
          Files
        </span>
      </div>
      <FileTree />
    </div>
  );
}
