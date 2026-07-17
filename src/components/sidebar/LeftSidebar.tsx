import { useState, useEffect } from "react";
import { FolderPlus, Folder, Server, ChevronRight, ChevronDown, Trash2 } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import AddProjectDialog from "../dialogs/AddProjectDialog";
import { Project } from "../../types";

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
  return (
    <div>
      <div
        className={`group flex items-center gap-1.5 px-3 py-1.5 text-sm cursor-pointer transition-colors ${
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
        {project.type === "local" ? (
          <Folder size={14} className="text-[var(--color-blue)] shrink-0" />
        ) : (
          <Server size={14} className="text-[var(--color-mauve)] shrink-0" />
        )}
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
          {project.worktrees.length === 0 && (
            <div className="px-3 py-1 text-xs text-[var(--color-overlay0)]">No worktrees</div>
          )}
          {project.worktrees.map((wt) => (
            <div
              key={wt.id}
              className="flex items-center gap-2 px-3 py-1 text-xs text-[var(--color-subtext0)]"
            >
              <span
                className={`h-2 w-2 rounded-full ${
                  wt.status === "clean"
                    ? "bg-[var(--color-green)]"
                    : wt.status === "dirty"
                      ? "bg-[var(--color-yellow)]"
                      : "bg-[var(--color-overlay0)]"
                }`}
              />
              {wt.branch}
              {wt.isMain && <span className="text-[var(--color-overlay0)]">(main)</span>}
            </div>
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
