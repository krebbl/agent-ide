import { useState, useEffect, useRef, useCallback } from "react";
import { FolderPlus, Folder, Server, ChevronRight, ChevronDown, Trash2, Loader2, GitBranch, CircleDot, ArrowUp, ArrowDown, MoreVertical, Terminal, FolderOpen, Copy, CopyCheck, RefreshCw, Plus } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import { useConnectionStatusStore } from "../../stores/connectionStatusStore";
import { useTerminalStore } from "../../stores/terminalStore";
import AddProjectDialog from "../dialogs/AddProjectDialog";
import AddWorktreeDialog from "../dialogs/AddWorktreeDialog";
import { Project } from "../../types";

function WorktreeContextMenu({
  worktree,
  projectId,
  projectType,
  onClose,
  onRemove,
}: {
  worktree: { id: string; branch: string; path: string; isMain: boolean; status: string; ahead: number; behind: number };
  projectId: string;
  projectType: "local" | "ssh";
  onClose: () => void;
  onRemove: (force: boolean, deleteBranch: boolean) => void;
}) {
  const [copied, setCopied] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);
  const [deleteBranch, setDeleteBranch] = useState(false);

  const handleCopyPath = async () => {
    try {
      await navigator.clipboard.writeText(worktree.path);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // ignore
    }
  };

  const handleOpenInTerminal = () => {
    useTerminalStore
      .getState()
      .addSession(worktree.path, projectType, projectId, worktree.id)
      .catch(() => {});
    onClose();
  };

  const handleOpenInFileManager = () => {
    // TODO: open file manager at worktree.path
    onClose();
  };

  if (showConfirm) {
    return (
      <div className="fixed inset-0 z-[60]" onClick={onClose}>
        <div
          className="absolute z-[61] rounded-md border border-[var(--color-surface0)] bg-[var(--color-mantle)] p-3 shadow-xl"
          onClick={(e) => e.stopPropagation()}
          style={{ top: "50%", left: "50%", transform: "translate(-50%, -50%)" }}
        >
          <p className="mb-2 text-xs text-[var(--color-text)]">
            Remove worktree <span className="font-mono">{worktree.path}</span>?
          </p>
          {worktree.status === "dirty" && (
            <p className="mb-2 text-xs text-[var(--color-yellow)]">
              This worktree has uncommitted changes.
            </p>
          )}
          <label className="mb-3 flex items-start gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={deleteBranch}
              onChange={(e) => setDeleteBranch(e.target.checked)}
              className="mt-0.5 accent-[var(--color-blue)]"
            />
            <span className="text-xs text-[var(--color-subtext1)]">
              Also delete branch <span className="font-mono">{worktree.branch}</span>
            </span>
          </label>
          <div className="flex gap-2">
            <button
              onClick={() => { onRemove(false, deleteBranch); onClose(); }}
              disabled={worktree.status === "dirty"}
              className="rounded-md bg-[var(--color-red)]/20 px-3 py-1 text-xs text-[var(--color-red)] hover:bg-[var(--color-red)]/30 disabled:opacity-50"
            >
              Remove
            </button>
            {worktree.status === "dirty" && (
              <button
                onClick={() => { onRemove(true, deleteBranch); onClose(); }}
                className="rounded-md bg-[var(--color-peach)]/20 px-3 py-1 text-xs text-[var(--color-peach)] hover:bg-[var(--color-peach)]/30"
              >
                Force Remove
              </button>
            )}
            <button
              onClick={() => { setShowConfirm(false); }}
              className="rounded-md bg-[var(--color-surface0)] px-3 py-1 text-xs text-[var(--color-overlay1)] hover:bg-[var(--color-surface1)]"
            >
              Cancel
            </button>
          </div>
        </div>
      </div>
    );
  }

  const isDisabled = worktree.isMain;
  const disableReason = "Cannot remove the main worktree";

  return (
    <div className="absolute z-50 min-w-[180px] rounded-md border border-[var(--color-surface0)] bg-[var(--color-mantle)] py-1 shadow-xl">
      <button
        onClick={handleOpenInTerminal}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-xs text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]"
      >
        <Terminal size={12} />
        Open in Terminal
      </button>
      <button
        onClick={handleOpenInFileManager}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-xs text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]"
      >
        <FolderOpen size={12} />
        Open in File Manager
      </button>
      <button
        onClick={handleCopyPath}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-xs text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]"
      >
        {copied ? <CopyCheck size={12} className="text-[var(--color-green)]" /> : <Copy size={12} />}
        {copied ? "Copied!" : "Copy Path"}
      </button>
      <div className="my-1 border-t border-[var(--color-surface0)]" />
      <div className="relative group/remove">
        <button
          onClick={() => setShowConfirm(true)}
          disabled={isDisabled}
          className="flex w-full items-center gap-2 px-3 py-1.5 text-xs text-[var(--color-red)] hover:bg-[var(--color-surface0)] disabled:opacity-50 disabled:cursor-not-allowed"
          title={isDisabled ? disableReason : undefined}
        >
          <Trash2 size={12} />
          Remove Worktree
        </button>
      </div>
    </div>
  );
}

function WorktreeItem({
  worktree,
  projectId,
  projectType,
  isActive,
  onActivate,
  onRemove,
}: {
  worktree: { id: string; branch: string; path: string; isMain: boolean; status: string; ahead: number; behind: number };
  projectId: string;
  projectType: "local" | "ssh";
  isActive: boolean;
  onActivate: () => void;
  onRemove: (force: boolean, deleteBranch: boolean) => void;
}) {
  const [showMenu, setShowMenu] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const activity = useTerminalStore((s) => {
    return s.sessions.reduce<"idle" | "busy" | "input">((state, session) => {
      if (
        session.projectId === projectId &&
        session.worktreeId === worktree.id
      ) {
        if (session.processRunning || session.isBusy) return "busy";
        if (session.needsInput && state !== "busy") return "input";
      }
      return state;
    }, "idle");
  });

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setShowMenu(true);
  }, []);

  useEffect(() => {
    if (!showMenu) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setShowMenu(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [showMenu]);

  const worktreeName = worktree.isMain
    ? "local"
    : worktree.path.split(/[\\/]/).pop() || worktree.id;

  return (
    <div className="relative">
      <div
        className={`group flex items-start gap-1.5 px-3 py-1 cursor-pointer transition-colors ${
          isActive
            ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
            : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50"
        }`}
        onClick={onActivate}
        onContextMenu={handleContextMenu}
        title={worktree.path}
      >
        <div className="relative shrink-0 pt-0.5">
          {activity === "busy" ? (
            <Loader2 size={10} className="animate-spin text-[var(--color-blue)]" />
          ) : activity === "input" ? (
            <ChevronRight size={10} className="text-[var(--color-green)]" />
          ) : (
            <GitBranch size={10} className={worktree.isMain ? "text-[var(--color-blue)]" : "text-[var(--color-overlay1)]"} />
          )}
          {worktree.isMain && activity === "idle" && (
            <CircleDot size={6} className="absolute -bottom-0.5 -right-0.5 text-[var(--color-blue)]" />
          )}
        </div>
        <div className="flex flex-1 flex-col min-w-0">
          <span className="truncate text-xs font-medium">{worktreeName}</span>
          <span className="truncate text-[10px] text-[var(--color-overlay1)]">{worktree.branch}</span>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-0.5">
          <div className="flex items-center gap-1">
            {worktree.ahead > 0 && (
              <span className="flex items-center gap-0.5 text-[9px] text-[var(--color-green)]">
                <ArrowUp size={8} />
                {worktree.ahead}
              </span>
            )}
            {worktree.behind > 0 && (
              <span className="flex items-center gap-0.5 text-[9px] text-[var(--color-red)]">
                <ArrowDown size={8} />
                {worktree.behind}
              </span>
            )}
            <button
              onClick={(e) => {
                e.stopPropagation();
                setShowMenu(true);
              }}
              className="shrink-0 text-[var(--color-overlay0)] opacity-0 transition-opacity group-hover:opacity-100 hover:text-[var(--color-text)]"
            >
              <MoreVertical size={10} />
            </button>
          </div>
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              worktree.status === "clean"
                ? "bg-[var(--color-green)]"
                : worktree.status === "dirty"
                  ? "bg-[var(--color-yellow)]"
                  : "bg-[var(--color-overlay0)]"
            }`}
          />
        </div>
      </div>
      {showMenu && (
        <div ref={menuRef}>
          <WorktreeContextMenu
            worktree={worktree}
            projectId={projectId}
            projectType={projectType}
            onClose={() => setShowMenu(false)}
            onRemove={onRemove}
          />
        </div>
      )}
    </div>
  );
}

function ProjectItem({
  project,
  isActive,
  isExpanded,
  onToggle,
  onSelect,
  onRemove,
}: {
  project: Project;
  isActive: boolean;
  isExpanded: boolean;
  onToggle: () => void;
  onSelect: () => void;
  onRemove: () => void;
}) {
  const connectionStatus = useConnectionStatusStore((s) => s.statuses[project.id]?.status);
  const { fetchWorktrees, setActiveWorktree, worktreeLoading, removeWorktree, refreshWorktrees, selectedWorktreeId, activeProjectId } = useProjectStore();
  const isWorktreeLoading = worktreeLoading[project.id] ?? false;
  const [showAddDialog, setShowAddDialog] = useState(false);

  const statusColor =
    project.type !== "ssh"
      ? ""
      : connectionStatus === "connected"
        ? "bg-[var(--color-green)]"
        : connectionStatus === "reconnecting"
          ? "bg-[var(--color-yellow)]"
          : connectionStatus === "error"
            ? "bg-[var(--color-red)]"
            : "bg-[var(--color-overlay0)]";

  const fetchedRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    if (!isExpanded) {
      fetchedRef.current.delete(project.id);
    }
  }, [isExpanded, project.id]);

  useEffect(() => {
    if (isExpanded && !fetchedRef.current.has(project.id) && !isWorktreeLoading) {
      fetchedRef.current.add(project.id);
      fetchWorktrees(project.id);
    }
  }, [isExpanded, project.id, isWorktreeLoading, fetchWorktrees]);

  const handleRefresh = () => {
    fetchedRef.current.delete(project.id);
    refreshWorktrees(project.id);
  };

  const handleRemoveWorktree = async (worktreePath: string, force: boolean, deleteBranch: boolean) => {
    await removeWorktree(project.id, worktreePath, force, deleteBranch);
  };

  return (
    <div>
      <div
        className={`group relative flex items-center gap-1.5 px-3 py-1.5 text-sm cursor-pointer transition-colors ${
          isActive
            ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
            : "text-[var(--color-subtext1)] hover:bg-[var(--color-surface0)]/50"
        }`}
        onClick={onSelect}
      >
        <button
          onClick={(e) => {
            e.stopPropagation();
            onToggle();
          }}
          className="shrink-0 text-[var(--color-overlay1)] hover:text-[var(--color-text)]"
        >
          {isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </button>
        <div className="relative shrink-0">
          {project.type === "local" ? (
            <Folder size={14} className="text-[var(--color-blue)]" />
          ) : (
            <Server size={14} className="text-[var(--color-mauve)]" />
          )}
          {project.type === "ssh" && (
            <span
              className={`absolute -bottom-0.5 -right-0.5 h-2 w-2 rounded-full border border-[var(--color-crust)] ${statusColor}`}
            />
          )}
        </div>
        <span className="flex-1 truncate">{project.name}</span>
        <div className="flex shrink-0 items-center gap-1">
          <button
            onClick={(e) => {
              e.stopPropagation();
              setShowAddDialog(true);
            }}
            className="text-[var(--color-overlay0)] hover:text-[var(--color-blue)]"
            title="Add worktree"
          >
            <Plus size={12} />
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              handleRefresh();
            }}
            disabled={isWorktreeLoading}
            className="text-[var(--color-overlay0)] hover:text-[var(--color-text)] disabled:opacity-50"
            title="Refresh worktrees"
          >
            {isWorktreeLoading ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onRemove();
            }}
            className="text-[var(--color-overlay0)] opacity-0 transition-opacity group-hover:opacity-100 hover:text-[var(--color-red)]"
            title="Remove project"
          >
            <Trash2 size={12} />
          </button>
        </div>
      </div>
      {isExpanded && (
        <div className="ml-6 border-l border-[var(--color-surface0)] pl-2">
          {isWorktreeLoading && (
            <div className="flex items-center gap-1.5 px-3 py-1 text-xs text-[var(--color-overlay0)]">
              <Loader2 size={10} className="animate-spin" />
              Loading worktrees...
            </div>
          )}
          {!isWorktreeLoading && project.worktrees.length === 0 && (
            <div className="px-3 py-1 text-xs text-[var(--color-overlay0)]">No worktrees</div>
          )}
          {!isWorktreeLoading && project.worktrees.map((wt) => (
            <WorktreeItem
              key={wt.id}
              worktree={wt}
              projectId={project.id}
              projectType={project.type}
              isActive={wt.id === selectedWorktreeId && project.id === activeProjectId}
              onActivate={() => setActiveWorktree(project.id, wt.id)}
              onRemove={(force, deleteBranch) => handleRemoveWorktree(wt.path, force, deleteBranch)}
            />
          ))}
        </div>
      )}
      {showAddDialog && <AddWorktreeDialog projectId={project.id} onClose={() => setShowAddDialog(false)} />}
    </div>
  );
}

export default function LeftSidebar() {
  const {
    projects,
    activeProjectId,
    expandedProjectIds,
    setActiveProject,
    loadProjects,
    removeProject,
    toggleProjectExpanded,
  } = useProjectStore();
  const [showAddDialog, setShowAddDialog] = useState(false);

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  const handleToggle = (id: string) => {
    toggleProjectExpanded(id);
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center justify-between gap-2 border-b border-[var(--color-surface0)] px-3 min-w-0">
        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)] truncate">
          Projects
        </span>
        <button
          onClick={() => setShowAddDialog(true)}
          className="shrink-0 text-[var(--color-overlay1)] transition-colors hover:text-[var(--color-blue)]"
        >
          <FolderPlus size={16} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto py-1">
        {projects.length === 0 && (
          <div className="flex flex-1 items-center justify-center p-4">
            <span className="text-sm text-[var(--color-overlay0)]">No projects yet</span>
          </div>
        )}
        {projects.map((project) => (
          <ProjectItem
            key={project.id}
            project={project}
            isActive={project.id === activeProjectId}
            isExpanded={expandedProjectIds.has(project.id)}
            onToggle={() => handleToggle(project.id)}
            onSelect={() => setActiveProject(project.id)}
            onRemove={() => removeProject(project.id)}
          />
        ))}
      </div>

      {showAddDialog && <AddProjectDialog onClose={() => setShowAddDialog(false)} />}
    </div>
  );
}
