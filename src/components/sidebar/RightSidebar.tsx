import { useEffect, useRef } from "react";
import { useFileTreeStore } from "../../stores/fileTreeStore";
import { useEditorStore } from "../../stores/editorStore";
import { useProjectStore } from "../../stores/projectStore";
import { useConnectionStatusStore } from "../../stores/connectionStatusStore";
import FileTree from "./FileTree";
import JiraPanel from "./JiraPanel";
import LoadingOverlay from "../ui/LoadingOverlay";
import { useUiStore } from "../../stores/uiStore";

export default function RightSidebar() {
  const { setRoot } = useFileTreeStore();
  const { projects, activeProjectId } = useProjectStore();
  const connectionStatus = useConnectionStatusStore((s) =>
    activeProjectId ? s.statuses[activeProjectId]?.status : undefined,
  );
  const lastKey = useRef("");
  const loadSeq = useRef(0);
  const worktreeLoading = useUiStore((s) => s.worktreeLoading);
  const rightSidebarTab = useUiStore((s) => s.rightSidebarTab);
  const setRightSidebarTab = useUiStore((s) => s.setRightSidebarTab);

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

    if (activeProject.type === "ssh" && connectionStatus !== "connected") {
      lastKey.current = "";
      loadSeq.current++;
      useUiStore.getState().setWorktreeLoading(connectionStatus === undefined);
      return;
    }
    const statusSuffix = activeProject.type === "ssh" ? `:${connectionStatus ?? ""}` : "";
    const key = `${activeProject.id}:${activeProject.type}:${worktree.path}${statusSuffix}`;
    if (key === lastKey.current) return;
    lastKey.current = key;

    const seq = ++loadSeq.current;
    useUiStore.getState().setWorktreeLoading(true);
    const fallbackTimer = setTimeout(() => {
      if (loadSeq.current === seq) {
        useUiStore.getState().setWorktreeLoading(false);
      }
    }, 15000);
    void Promise.all([
      useEditorStore
        .getState()
        .setWorktree(`${activeProject.id}:${worktree.path}`, activeProject.id),
      setRoot(worktree.path, activeProject.id, activeProject.type),
    ])
      .catch(() => {})
      .finally(() => {
        clearTimeout(fallbackTimer);
        if (loadSeq.current === seq) {
          useUiStore.getState().setWorktreeLoading(false);
        }
      });
  }, [activeProjectId, projects, setRoot, connectionStatus]);

  const tabs: Array<{ id: "files" | "jira"; label: string }> = [
    { id: "files", label: "Files" },
    { id: "jira", label: "Jira" },
  ];

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center gap-3 border-b border-[var(--color-surface0)] px-3">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setRightSidebarTab(tab.id)}
            className={`text-xs font-semibold uppercase tracking-wide transition-colors ${
              rightSidebarTab === tab.id
                ? "text-[var(--color-blue)]"
                : "text-[var(--color-subtext1)] hover:text-[var(--color-text)]"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div
        className="relative flex-1 overflow-hidden"
        style={{ display: rightSidebarTab === "files" ? undefined : "none" }}
      >
        <FileTree />
        {worktreeLoading && <LoadingOverlay label="Loading files…" />}
      </div>
      <div
        className="flex-1 overflow-hidden"
        style={{ display: rightSidebarTab === "jira" ? undefined : "none" }}
      >
        <JiraPanel />
      </div>
    </div>
  );
}
