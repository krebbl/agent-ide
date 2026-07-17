import { useState, useEffect, useRef } from "react";
import { FolderPlus, Folder, Server, ChevronRight, ChevronDown, Trash2, Loader2, GitBranch, CircleDot, ArrowUp, ArrowDown } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import { useConnectionStatusStore } from "../../stores/connectionStatusStore";
import AddProjectDialog from "../dialogs/AddProjectDialog";
import { Project } from "../../types";

function WorktreeItem({
  worktree,
  isActive,
  onActivate,
}: {
  worktree: { id: string; branch: string; path: string; isMain: boolean; status: string; ahead: number; behind: number };
  isActive: boolean;
  onActivate: () => void;
}) {
  return (
    <div
      className={`group flex items-center gap-1.5 px-3 py-1 text-xs cursor-pointer transition-colors ${
        isActive
          ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
          : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50"
      }`}
      onClick={onActivate}
      title={worktree.path}
    >
      <div className="relative shrink-0">
        <GitBranch size={10} className={worktree.isMain ? "text-[var(--color-blue)]" : "text-[var(--color-overlay1)]"} />
        {worktree.isMain && (
          <CircleDot size={6} className="absolute -bottom-0.5 -right-0.5 text-[var(--color-blue)]" />
        )}
      </div>
      <span className="flex-1 truncate">{worktree.branch}</span>
      <span
        className={`h-1.5 w-1.5 shrink-0 rounded-full ${
          worktree.status === "clean"
            ? "bg-[var(--color-green)]"
            : worktree.status === "dirty"
              ? "bg-[var(--color-yellow)]"
              : "bg-[var(--color-overlay0)]"
        }`}
      />
      {worktree.ahead > 0 && (
        <span className="flex items-center gap-0.5 text-[var(--color-green)]">
          <ArrowUp size={8} />
          {worktree.ahead}
        </span>
      )}
      {worktree.behind > 0 && (
        <span className="flex items-center gap-0.5 text-[var(--color-peach)]">
          <ArrowDown size={8} />
          {worktree.behind}
        </span>
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
  const { fetchWorktrees, setActiveWorktree, worktreeLoading } = useProjectStore();
  const isLoading = worktreeLoading[project.id] ?? false;

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
    if (isExpanded && project.worktrees.length === 0 && !isLoading && !fetchedRef.current.has(project.id)) {
      fetchedRef.current.add(project.id);
      fetchWorktrees(project.id);
    }
  }, [isExpanded, project.id, project.worktrees.length, isLoading, fetchWorktrees]);

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
        <button
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="shrink-0 text-[var(--color-overlay0)] opacity-0 transition-opacity group-hover:opacity-100 hover:text-[var(--color-red)]"
        >
          <Trash2 size={12} />
        </button>
      </div>
      {isExpanded && (
        <div className="ml-6 border-l border-[var(--color-surface0)] pl-2">
          {isLoading && (
            <div className="flex items-center gap-1.5 px-3 py-1 text-xs text-[var(--color-overlay0)]">
              <Loader2 size={10} className="animate-spin" />
              Loading worktrees...
            </div>
          )}
          {!isLoading && project.worktrees.length === 0 && (
            <div className="px-3 py-1 text-xs text-[var(--color-overlay0)]">No worktrees</div>
          )}
          {!isLoading && project.worktrees.map((wt) => (
            <WorktreeItem
              key={wt.id}
              worktree={wt}
              isActive={wt.id === project.activeWorktreeId}
              onActivate={() => setActiveWorktree(project.id, wt.id)}
            />
          ))}
          <button className="w-full px-3 py-1 text-left text-xs text-[var(--color-blue)] hover:bg-[var(--color-surface0)]/50">
            + Add Worktree
          </button>
        </div>
      )}
    </div>
  );
}

export default function LeftSidebar() {
  const { projects, activeProjectId, setActiveProject, loadProjects, removeProject } =
    useProjectStore();
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [expandedProjects, setExpandedProjects] = useState<Set<string>>(new Set());

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  const handleToggle = (id: string) => {
    setExpandedProjects((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
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
            isExpanded={expandedProjects.has(project.id)}
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
