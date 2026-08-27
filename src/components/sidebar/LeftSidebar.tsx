import { useState, useEffect, useRef, useCallback, useMemo, Fragment } from "react";
import { FolderPlus, ChevronRight, ChevronDown, Trash2, Loader2, GitBranch, CircleDot, ArrowUp, ArrowDown, Bot, Terminal, FolderOpen, Copy, CopyCheck, RefreshCw, Plus, GitPullRequest, GitPullRequestClosed, GitPullRequestDraft, GitMerge, BrushCleaning } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import { useConnectionStatusStore } from "../../stores/connectionStatusStore";
import { useTerminalStore } from "../../stores/terminalStore";
import { usePrStore } from "../../stores/prStore";
import AddProjectDialog from "../dialogs/AddProjectDialog";
import AddWorktreeDialog from "../dialogs/AddWorktreeDialog";
import NewAgentSessionDialog from "../dialogs/NewAgentSessionDialog";
import { Project, PrInfo, Worktree } from "../../types";
import Dialog from "../ui/Dialog";
import { openUrl } from "../../utils/openUrl";
import { useSortable } from "@dnd-kit/sortable";
import {
  DndContext,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";

function WorktreeContextMenu({
  worktree,
  projectId,
  projectType,
  onClose,
  onRemove,
  onStartAgent,
}: {
  worktree: { id: string; branch: string; path: string; isMain: boolean; status: string; ahead: number; behind: number; locked: boolean };
  projectId: string;
  projectType: "local" | "ssh";
  onClose: () => void;
  onRemove: (force: boolean, deleteBranch: boolean) => Promise<void>;
  onStartAgent: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);
  const [deleteBranch, setDeleteBranch] = useState(false);
  const [removeError, setRemoveError] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);

  const handleRemove = async (force: boolean) => {
    setRemoving(true);
    setRemoveError(null);
    try {
      await onRemove(force, deleteBranch);
      onClose();
    } catch (e) {
      setRemoveError(String(e));
    } finally {
      setRemoving(false);
    }
  };

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
          {worktree.locked && (
            <p className="mb-2 text-xs text-[var(--color-yellow)]">
              This worktree is locked. Removing it will override the lock.
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
          {removeError && (
            <p className="mb-2 max-w-xs text-xs text-[var(--color-red)]">{removeError}</p>
          )}
          <div className="flex gap-2">
            <button
              onClick={() => handleRemove(false)}
              disabled={worktree.status === "dirty" || worktree.locked || removing}
              className="rounded-md bg-[var(--color-red)]/20 px-3 py-1 text-xs text-[var(--color-red)] hover:bg-[var(--color-red)]/30 disabled:opacity-50"
            >
              Remove
            </button>
            {(worktree.status === "dirty" || worktree.locked || removeError) && (
              <button
                onClick={() => handleRemove(true)}
                disabled={removing}
                className="rounded-md bg-[var(--color-peach)]/20 px-3 py-1 text-xs text-[var(--color-peach)] hover:bg-[var(--color-peach)]/30 disabled:opacity-50"
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
        onClick={() => {
          onStartAgent();
          onClose();
        }}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-xs text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]"
      >
        <Bot size={12} />
        Start Agent Session
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

function getPrStatusRank(pr: PrInfo | null | undefined): number {
  if (!pr) return 0;
  if (pr.state === "merged") return 2;
  if (pr.state === "closed") return 3;
  return 1;
}

function getWorktreeActivity(
  sessions: import("../../stores/terminalStore").TerminalSession[],
  projectId: string,
  worktreeId: string,
): "idle" | "busy" | "input" | "unseen" {
  return sessions.reduce<"idle" | "busy" | "input" | "unseen">((state, session) => {
    if (session.projectId === projectId && session.worktreeId === worktreeId) {
      if (session.processRunning || session.isBusy) return "busy";
      if (session.hasUnseenActivity) return "unseen";
      if (session.needsInput && state !== "busy" && state !== "unseen") return "input";
    }
    return state;
  }, "idle");
}

function WorktreeItem({
  worktree,
  projectId,
  projectType,
  isActive,
  onActivate,
  onRemove,
  onStartAgent,
}: {
  worktree: { id: string; branch: string; path: string; isMain: boolean; status: string; ahead: number; behind: number; locked: boolean };
  projectId: string;
  projectType: "local" | "ssh";
  isActive: boolean;
  onActivate: () => void;
  onRemove: (force: boolean, deleteBranch: boolean) => Promise<void>;
  onStartAgent: () => void;
}) {
  const prEntry = usePrStore((s) => s.cache[`${projectId}:${worktree.branch}`]);
  const prText = prEntry?.pr ? `#${prEntry.pr.number}` : null;
  const prColor = !prEntry?.pr
    ? ""
    : prEntry.pr.checkStatus === "success"
      ? "text-[var(--color-green)]"
      : prEntry.pr.checkStatus === "failure"
        ? "text-[var(--color-red)]"
        : prEntry.pr.checkStatus === "pending"
          ? "text-[var(--color-yellow)]"
          : "text-[var(--color-subtext1)]";
  const [showMenu, setShowMenu] = useState(false);
  const [showInfo, setShowInfo] = useState(false);
  const [infoPos, setInfoPos] = useState({ top: 0, left: 0 });
  const itemRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const activity = useTerminalStore((s) =>
    getWorktreeActivity(s.sessions, projectId, worktree.id),
  );
  const terminalCount = useTerminalStore(
    (s) =>
      s.sessions.filter(
        (session) =>
          session.projectId === projectId && session.worktreeId === worktree.id,
      ).length,
  );

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
        ref={itemRef}
        className={`group relative flex items-start gap-1.5 px-3 py-1 cursor-pointer transition-colors ${
          isActive
            ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
            : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50"
        }`}
        onClick={onActivate}
        onContextMenu={handleContextMenu}
        onMouseEnter={() => {
          if (itemRef.current) {
            const rect = itemRef.current.getBoundingClientRect();
            setInfoPos({ top: rect.top, left: rect.right + 8 });
          }
          setShowInfo(true);
        }}
        onMouseLeave={() => setShowInfo(false)}
      >
        <div className="relative flex shrink-0 items-center gap-0.5 pt-0.5">
          {activity === "busy" ? (
            <span className="flex w-[18px] justify-center">
              <Loader2 size={10} className="animate-spin text-[var(--color-blue)]" />
            </span>
          ) : activity === "unseen" ? (
            <span
              className="animate-blink flex w-[18px] justify-center text-[10px] font-bold text-[var(--color-green)]"
              title="Terminal ready — unseen output"
            >
              !
            </span>
          ) : terminalCount > 0 ? (
            <span
              className={`w-[18px] py-px text-center text-[9px] leading-none ${
                activity === "input"
                  ? "text-[var(--color-blue)]"
                  : "text-[var(--color-overlay1)]"
              }`}
              title={`${terminalCount} terminal${terminalCount > 1 ? "s" : ""} open`}
            >
              {terminalCount}
            </span>
          ) : (
            <span className="w-[18px]" />
          )}
          <div className="relative">
            <>
              {prEntry?.pr && prEntry.pr.state === "open" && <GitPullRequest size={10} className="text-[var(--color-green)]" />}
              {prEntry?.pr && prEntry.pr.state === "merged" && <GitMerge size={10} className="text-[var(--color-mauve)]" />}
              {prEntry?.pr && prEntry.pr.state === "closed" && <GitPullRequestClosed size={10} className="text-[var(--color-red)]" />}
              {prEntry?.pr && prEntry.pr.state === "draft" && <GitPullRequestDraft size={10} className="text-[var(--color-overlay1)]" />}
              {(!prEntry?.pr) && (
                <>
                  <GitBranch size={10} className={worktree.isMain ? "text-[var(--color-blue)]" : "text-[var(--color-overlay1)]"} />
                  {worktree.isMain && (
                    <CircleDot size={6} className="absolute -bottom-0.5 -right-0.5 text-[var(--color-blue)]" />
                  )}
                </>
              )}
            </>
          </div>
        </div>
        <div className="flex flex-1 flex-col min-w-0">
          <span className="truncate text-xs font-medium">{worktreeName}</span>
          <span className="truncate text-[10px] text-[var(--color-overlay1)]">{worktree.branch}</span>
        </div>
        <div className="flex shrink-0 flex-col items-end justify-between gap-0.5 self-stretch">
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
          </div>
          {prText && prEntry?.pr && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                openUrl(prEntry.pr!.url).catch(() => {});
              }}
              className={`text-[10px] ${prColor} hover:text-[var(--color-blue)] hover:underline`}
              title={prEntry.pr.title}
            >
              {prText}
            </button>
          )}
        </div>

      </div>
      {showInfo && (
        <div
          className="pointer-events-none fixed z-40 rounded-md border border-[var(--color-surface0)] bg-[var(--color-mantle)] px-2.5 py-1.5 shadow-lg whitespace-nowrap"
          style={{ top: infoPos.top, left: infoPos.left }}
        >
          <span className="block text-xs font-medium text-[var(--color-text)]">{worktreeName}</span>
          <span className="block text-[10px] text-[var(--color-overlay1)]">{worktree.branch}</span>
        </div>
      )}
      {showMenu && (
        <div ref={menuRef}>
          <WorktreeContextMenu
            worktree={worktree}
            projectId={projectId}
            projectType={projectType}
            onClose={() => setShowMenu(false)}
            onRemove={onRemove}
            onStartAgent={onStartAgent}
          />
        </div>
      )}
    </div>
  );
}

function CleanupWorktreesDialog({
  projectName,
  target,
  branches,
  error,
  loading,
  onConfirm,
  onCancel,
}: {
  projectName: string;
  target: string;
  branches: string[];
  error: string | null;
  loading: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Dialog
      title="Cleanup merged worktrees"
      icon={<BrushCleaning size={16} className="text-[var(--color-mauve)]" />}
      danger
      width="400px"
      onClose={onCancel}
      footer={
        <>
          <button
            onClick={onCancel}
            disabled={loading}
            className="rounded-md px-4 py-2 text-sm text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)] disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={loading || branches.length === 0}
            className="rounded-md bg-[var(--color-red)] px-4 py-2 text-sm font-medium text-[var(--color-crust)] hover:bg-[var(--color-red)]/80 disabled:opacity-50"
          >
            {loading ? "Cleaning..." : "Cleanup"}
          </button>
        </>
      }
    >
      {branches.length > 0 ? (
        <>
          <p className="mb-3 text-sm text-[var(--color-text)]">
            Remove {branches.length} merged worktree{branches.length > 1 ? "s" : ""} from{" "}
            <span className="font-medium">{projectName}</span> and close their terminal sessions?
          </p>
          <ul className="max-h-40 list-inside list-disc overflow-y-auto rounded-md bg-[var(--color-base)] p-2 text-xs text-[var(--color-subtext0)]">
            {branches.map((branch) => (
              <li key={branch}>{branch}</li>
            ))}
          </ul>
          <p className="mt-3 text-xs text-[var(--color-overlay0)]">
            Branches with <span className="font-medium">{target}</span>. The associated local branches will also be deleted.
          </p>
        </>
      ) : (
        <p className="text-sm text-[var(--color-text)]">
          No worktrees with <span className="font-medium">{target}</span> to clean up for{" "}
          <span className="font-medium">{projectName}</span>.
        </p>
      )}
      {error && (
        <p className="mt-3 whitespace-pre-line text-xs text-[var(--color-red)]">{error}</p>
      )}
    </Dialog>
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
  const { attributes, listeners, setNodeRef, isDragging } =
    useSortable({ id: project.id });
  const connectionStatus = useConnectionStatusStore((s) => s.statuses[project.id]?.status);
  const { fetchWorktrees, setActiveWorktree, worktreeLoading, removeWorktree, refreshWorktrees, cleanupMergedWorktrees, selectedWorktreeId, activeProjectId } = useProjectStore();
  const { fetchPrsForWorktrees } = usePrStore();
  const prCache = usePrStore((s) => s.cache);
  const isWorktreeLoading = worktreeLoading[project.id] ?? false;
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [showAgentDialog, setShowAgentDialog] = useState(false);
  const [agentWorktreeId, setAgentWorktreeId] = useState<string | undefined>(undefined);
  const [showCleanupDialog, setShowCleanupDialog] = useState(false);
  const [cleanupCandidates, setCleanupCandidates] = useState<Worktree[]>([]);
  const [cleanupTarget, setCleanupTarget] = useState<string>("");
  const [cleanupLoading, setCleanupLoading] = useState(false);
  const [cleanupError, setCleanupError] = useState<string | null>(null);

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

  const isConnected = project.type !== "ssh" || connectionStatus === "connected";

  useEffect(() => {
    if (!isExpanded || !isConnected) return;
    if (project.worktrees.length === 0 && !isWorktreeLoading) {
      fetchWorktrees(project.id);
    }
  }, [isExpanded, isConnected, project.id, project.worktrees.length, isWorktreeLoading, fetchWorktrees]);

  const handleRefresh = () => {
    refreshWorktrees(project.id);
  };

  useEffect(() => {
    if (!isExpanded || !isConnected || isWorktreeLoading || project.worktrees.length === 0) return;
    const branches = project.worktrees.map((w) => w.branch);
    fetchPrsForWorktrees(project.id, branches);
  }, [isExpanded, isConnected, isWorktreeLoading, project.worktrees, project.id, fetchPrsForWorktrees]);

  useEffect(() => {
    if (!isExpanded || !isConnected) return;
    const intervalMs = project.type === "ssh" ? 180_000 : 60_000;
    const id = setInterval(() => {
      const worktrees =
        useProjectStore.getState().projects.find((p) => p.id === project.id)?.worktrees ?? [];
      if (worktrees.length === 0) return;
      fetchPrsForWorktrees(project.id, worktrees.map((w) => w.branch), true);
    }, intervalMs);
    return () => clearInterval(id);
  }, [isExpanded, isConnected, project.id, project.type, fetchPrsForWorktrees]);

  const sortedWorktrees = useMemo(() => {
    return [...project.worktrees].sort((a, b) => {
      if (a.isMain !== b.isMain) return a.isMain ? -1 : 1;
      const aPr = prCache[`${project.id}:${a.branch}`]?.pr;
      const bPr = prCache[`${project.id}:${b.branch}`]?.pr;
      return (
        getPrStatusRank(aPr) - getPrStatusRank(bPr) ||
        a.branch.localeCompare(b.branch)
      );
    });
  }, [project.worktrees, project.id, prCache]);

  const hasMergedWorktrees = useMemo(
    () =>
      project.worktrees.some(
        (wt) => !wt.isMain && prCache[`${project.id}:${wt.branch}`]?.pr?.state === "merged",
      ),
    [project.worktrees, project.id, prCache],
  );

  const handleCleanupOpen = async () => {
    setCleanupError(null);
    setCleanupCandidates([]);
    setCleanupTarget("");
    setCleanupLoading(true);
    try {
      const branchesToCheck = project.worktrees
        .filter((w) => !w.isMain)
        .map((w) => w.branch);
      await fetchPrsForWorktrees(project.id, branchesToCheck, true);
      const prCache = usePrStore.getState().cache;
      const candidates = sortedWorktrees.filter(
        (wt) => !wt.isMain && prCache[`${project.id}:${wt.branch}`]?.pr?.state === "merged",
      );
      setCleanupCandidates(candidates);
      setCleanupTarget("merged PRs");
    } catch (e) {
      setCleanupError(String(e));
    } finally {
      setCleanupLoading(false);
      setShowCleanupDialog(true);
    }
  };

  const handleCleanup = async () => {
    setCleanupError(null);
    setCleanupLoading(true);
    try {
      await cleanupMergedWorktrees(project.id);
      setShowCleanupDialog(false);
      setCleanupCandidates([]);
    } catch (e) {
      setCleanupError(String(e));
    } finally {
      setCleanupLoading(false);
    }
  };

  const handleRemoveWorktree = async (worktreePath: string, force: boolean, deleteBranch: boolean) => {
    await removeWorktree(project.id, worktreePath, force, deleteBranch);
  };

  return (
    <>
      <div
        ref={setNodeRef}
        {...attributes}
        {...listeners}
        className={`group relative sticky top-0 z-10 flex items-center gap-1.5 bg-[var(--color-base)] px-3 py-1.5 text-sm cursor-pointer transition-colors select-none ${
          isActive
            ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
            : "text-[var(--color-subtext1)] hover:bg-[var(--color-surface0)]/50"
        } ${isDragging ? "opacity-50 z-10" : ""}`}
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
        <span className="flex min-w-0 flex-1 items-center gap-1.5 truncate text-xs font-bold">
          <span className="truncate">{project.name}</span>
          {project.type === "ssh" && (
            <span
              className={`h-2 w-2 shrink-0 rounded-full ${statusColor || "bg-[var(--color-overlay0)]"}`}
            />
          )}
        </span>
        <div className="flex shrink-0 items-center gap-1">
          <button
            onClick={(e) => {
              e.stopPropagation();
              setAgentWorktreeId(undefined);
              setShowAgentDialog(true);
            }}
            className="text-[var(--color-overlay0)] hover:text-[var(--color-mauve)]"
            title="Start agent session"
          >
            <Bot size={12} />
          </button>
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
          {hasMergedWorktrees && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                handleCleanupOpen();
              }}
              disabled={isWorktreeLoading || sortedWorktrees.length <= 1}
              className="text-[var(--color-overlay0)] hover:text-[var(--color-mauve)] disabled:opacity-50"
              title="Cleanup merged worktrees"
            >
              {cleanupLoading ? (
                <Loader2 size={12} className="animate-spin" />
              ) : (
                <BrushCleaning size={12} />
              )}
            </button>
          )}
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
        <div className="border-l border-[var(--color-surface0)]">
          {isWorktreeLoading && project.worktrees.length === 0 && (
            <div className="flex items-center gap-1.5 px-3 py-1 text-xs text-[var(--color-overlay0)]">
              <Loader2 size={10} className="animate-spin" />
              Loading worktrees...
            </div>
          )}
          {!isWorktreeLoading && project.worktrees.length === 0 && (
            <div className="px-3 py-1 text-xs text-[var(--color-overlay0)]">No worktrees</div>
          )}
          {sortedWorktrees.map((wt) => (
            <WorktreeItem
              key={wt.id}
              worktree={wt}
              projectId={project.id}
              projectType={project.type}
              isActive={wt.id === selectedWorktreeId && project.id === activeProjectId}
              onActivate={() => {
                const tStore = useTerminalStore.getState();
                tStore.sessions
                  .filter((s) => s.worktreeId === wt.id && s.hasUnseenActivity)
                  .forEach((s) => tStore.markSessionSeen(s.id));
                setActiveWorktree(project.id, wt.id);
              }}
              onRemove={(force, deleteBranch) => handleRemoveWorktree(wt.path, force, deleteBranch)}
              onStartAgent={() => {
                setAgentWorktreeId(wt.id);
                setShowAgentDialog(true);
              }}
            />
          ))}
        </div>
      )}
      {showAddDialog && <AddWorktreeDialog projectId={project.id} onClose={() => setShowAddDialog(false)} />}
      {showAgentDialog && (
        <NewAgentSessionDialog
          projectId={project.id}
          initialWorktreeId={agentWorktreeId}
          onClose={() => setShowAgentDialog(false)}
        />
      )}
      {showCleanupDialog && (
        <CleanupWorktreesDialog
          projectName={project.name}
          target={cleanupTarget}
          branches={cleanupCandidates.map((wt) => wt.branch)}
          error={cleanupError}
          loading={cleanupLoading}
          onConfirm={handleCleanup}
          onCancel={() => setShowCleanupDialog(false)}
        />
      )}
    </>
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
    reorderProjects,
  } = useProjectStore();
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [dragActiveId, setDragActiveId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } })
  );

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  useEffect(() => {
    const refresh = () => {
      const { projects, expandedProjectIds } = useProjectStore.getState();
      const statuses = useConnectionStatusStore.getState().statuses;
      const { fetchPrsForWorktrees, lastFetchedAt } = usePrStore.getState();
      for (const p of projects) {
        if (!expandedProjectIds.has(p.id)) continue;
        if (p.type === "ssh" && statuses[p.id]?.status !== "connected") continue;
        if (p.worktrees.length === 0) continue;
        if (Date.now() - (lastFetchedAt[p.id] ?? 0) < 15_000) continue;
        fetchPrsForWorktrees(p.id, p.worktrees.map((w) => w.branch), true);
      }
    };

    let unlisten: (() => void) | undefined;
    const setup = async () => {
      if (import.meta.env.VITE_TAURI === "true") {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        unlisten = await getCurrentWindow().listen("tauri://focus", refresh);
      } else {
        const handler = () => {
          if (!document.hidden) refresh();
        };
        document.addEventListener("visibilitychange", handler);
        unlisten = () => document.removeEventListener("visibilitychange", handler);
      }
    };
    setup().catch(() => {});
    return () => unlisten?.();
  }, []);

  const handleToggle = (id: string) => {
    toggleProjectExpanded(id);
  };

  const handleDragStart = (event: import("@dnd-kit/core").DragStartEvent) => {
    setDragActiveId(event.active.id?.toString() ?? null);
  };

  const handleDragOver = (event: import("@dnd-kit/core").DragOverEvent) => {
    setDragOverId(event.over?.id?.toString() ?? null);
  };

  const handleDragEnd = (event: import("@dnd-kit/core").DragEndEvent) => {
    setDragActiveId(null);
    setDragOverId(null);
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const fromIndex = projects.findIndex((p) => p.id === active.id);
    const toIndex = projects.findIndex((p) => p.id === over.id);
    if (fromIndex !== -1 && toIndex !== -1) {
      reorderProjects(fromIndex, toIndex);
    }
  };

  const [sessionTick, setSessionTick] = useState(0);
  useEffect(() => {
    return useTerminalStore.subscribe(() => {
      setSessionTick((t) => t + 1);
    });
  }, []);

  const sessions = useTerminalStore.getState().sessions;
  const projectNameById = new Map(projects.map((p) => [p.id, p.name]));
  const branchById = new Map(
    projects.flatMap((p) =>
      p.worktrees.map((w) => [`${p.id}:${w.id}`, w.branch]),
    ),
  );

  // React to new busy/agent store updates AND to time passing: a session
  // that stays busy crosses the long-running threshold without any store
  // mutation, so re-evaluate while any candidate session exists.
  useEffect(() => {
    const id = setInterval(() => {
      const s = useTerminalStore.getState().sessions;
      const hasCandidate = s.some(
        (x) => x.projectId && (x.agentActive || x.isBusy || x.processRunning),
      );
      if (hasCandidate) setSessionTick((t) => t + 1);
    }, 1000);
    return () => clearInterval(id);
  }, []);

  const ACTIVE_BUSY_THRESHOLD_MS = 5000;
  const activeSessions = sessions
    .filter((s) => s.projectId)
    .filter((s) => {
      if (s.agentActive) return true;
      if (!s.isBusy && !s.processRunning) return false;
      const since = s.busySince ?? 0;
      return Date.now() - since >= ACTIVE_BUSY_THRESHOLD_MS;
    })
    .sort((a, b) => {
      const aBranch =
        branchById.get(`${a.projectId}:${a.worktreeId}`) ?? "";
      const bBranch =
        branchById.get(`${b.projectId}:${b.worktreeId}`) ?? "";
      return (
        aBranch.localeCompare(bBranch) ||
        (a.createdAt ?? 0) - (b.createdAt ?? 0) ||
        a.title.localeCompare(b.title)
      );
    });
  void sessionTick;

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragStart={handleDragStart}
      onDragOver={handleDragOver}
      onDragEnd={handleDragEnd}
    >
      <div className="flex h-full flex-col">
        {activeSessions.length > 0 ? (
          <div className="border-b border-[var(--color-surface0)] py-1">
            <div className="px-3 py-1 text-[10px] font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
              Active
            </div>
            {activeSessions.map((session) => {
              const projectName =
                projectNameById.get(session.projectId ?? "") ?? "";
              const isAgent = session.agentActive === true;
              const branch =
                branchById.get(
                  `${session.projectId}:${session.worktreeId}`,
                ) ?? "";
              return (
                <button
                  key={`agent:${session.id}`}
                  onClick={() => {
                    const tStore = useTerminalStore.getState();
                    if (session.hasUnseenActivity) {
                      tStore.markSessionSeen(session.id);
                    }
                    tStore.focusSession(session.id);
                  }}
                  className="flex w-full items-start gap-2 px-3 py-1 text-left text-xs text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50"
                  title={`${isAgent ? session.agentName ?? "agent" : "terminal"} — ${session.title}${projectName ? ` — ${projectName}` : ""}${session.isBusy || session.processRunning ? " (busy)" : ""}`}
                >
                  {isAgent ? (
                    <Bot size={10} className="mt-0.5 shrink-0 text-[var(--color-mauve)]" />
                  ) : (
                    <Terminal size={10} className={`mt-0.5 shrink-0 ${session.isBusy || session.processRunning ? "text-[var(--color-blue)]" : "text-[var(--color-overlay1)]"}`} />
                  )}
                  <span className="min-w-0 flex-1">
                    <span className="flex w-full items-center gap-2">
                      <span className="truncate">{session.title}</span>
                      {projectName && (
                        <span className="shrink-0 text-[10px] text-[var(--color-overlay1)]">
                          {projectName}
                        </span>
                      )}
                    </span>
                    {branch && (
                      <span className="block truncate text-[10px] text-[var(--color-overlay1)]">
                        {branch}
                      </span>
                    )}
                  </span>
                </button>
              );
            })}
          </div>
        ) : null}
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

        <SortableContext
          items={projects.map((p) => p.id)}
          strategy={verticalListSortingStrategy}
        >
          <div className="flex-1 overflow-y-auto py-1 select-none">
            {projects.length === 0 && (
              <div className="flex flex-1 items-center justify-center p-4">
                <span className="text-sm text-[var(--color-overlay0)]">No projects yet</span>
              </div>
            )}
            {projects.map((project, i) => {
              const activeIdx = dragActiveId
                ? projects.findIndex((p) => p.id === dragActiveId)
                : -1;
              const showAbove =
                dragOverId === project.id && activeIdx !== -1 && activeIdx > i;
              const showBelow =
                dragOverId === project.id && activeIdx !== -1 && activeIdx < i;

              return (
                <Fragment key={project.id}>
                  {showAbove && (
                    <div className="h-0.5 bg-[var(--color-blue)]" key="above" />
                  )}
                  <div key={project.id}>
                    <ProjectItem
                      project={project}
                      isActive={project.id === activeProjectId}
                      isExpanded={expandedProjectIds.has(project.id)}
                      onToggle={() => handleToggle(project.id)}
                      onSelect={() => setActiveProject(project.id)}
                      onRemove={() => removeProject(project.id)}
                    />
                  </div>
                  {showBelow && (
                    <div className="h-0.5 bg-[var(--color-blue)]" key="below" />
                  )}
                </Fragment>
              );
            })}
          </div>
        </SortableContext>

        {showAddDialog && <AddProjectDialog onClose={() => setShowAddDialog(false)} />}
      </div>
    </DndContext>
  );
}
