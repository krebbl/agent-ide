import { useEffect, useMemo, useRef, useState } from "react";
import { GitBranch, Loader2 } from "lucide-react";
import Dialog from "../ui/Dialog";
import { useProjectStore } from "../../stores/projectStore";
import { useTerminalStore } from "../../stores/terminalStore";
import { usePrStore } from "../../stores/prStore";
import { Worktree } from "../../types";
import PrBadge from "../ui/PrBadge";
import { getWorktreeActivity } from "../../utils/worktreeActivity";

interface WorktreeSearchDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

interface Entry {
  projectId: string;
  projectName: string;
  worktree: Worktree;
  name: string;
}

export default function WorktreeSearchDialog({
  isOpen,
  onClose,
}: WorktreeSearchDialogProps) {
  const projects = useProjectStore((s) => s.projects);
  const activeProjectId = useProjectStore((s) => s.activeProjectId);
  const selectedWorktreeId = useProjectStore((s) => s.selectedWorktreeId);
  const sessions = useTerminalStore((s) => s.sessions);
  const prCache = usePrStore((s) => s.cache);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Active worktree first, then the active project's remaining worktrees
  // (main first), then other projects in store order.
  const worktrees = useMemo(() => {
    const entries: Entry[] = projects.flatMap((p) =>
      p.worktrees.map((w) => ({
        projectId: p.id,
        projectName: p.name,
        worktree: w,
        name: w.isMain ? "local" : w.path.split(/[\\/]/).pop() || w.id,
      })),
    );
    const rank = (e: Entry) => {
      const isActive =
        e.projectId === activeProjectId && e.worktree.id === selectedWorktreeId;
      return [
        isActive ? 0 : 1,
        e.projectId === activeProjectId ? 0 : 1,
        e.worktree.isMain ? 0 : 1,
      ];
    };
    return entries.sort((a, b) => {
      const ra = rank(a);
      const rb = rank(b);
      return ra[0] - rb[0] || ra[1] - rb[1] || ra[2] - rb[2];
    });
  }, [projects, activeProjectId, selectedWorktreeId]);

  const filtered = useMemo(() => {
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    if (terms.length === 0) return worktrees;
    return worktrees.filter((e) => {
      const fields = [e.projectName, e.worktree.branch, e.name].map((f) =>
        f.toLowerCase(),
      );
      const pr = prCache[`${e.projectId}:${e.worktree.branch}`]?.pr;
      if (pr) fields.push(`#${pr.number}`);
      return terms.every((t) => fields.some((f) => f.includes(t)));
    });
  }, [worktrees, query, prCache]);

  // Reset only when the dialog opens: query cleared, currently active
  // worktree preselected. Keyed on `isOpen` alone — the list is read
  // through a ref so busy-state updates don't clear the query.
  const worktreesRef = useRef(worktrees);
  worktreesRef.current = worktrees;
  useEffect(() => {
    if (!isOpen) return;
    setQuery("");
    const list = worktreesRef.current;
    const activeIdx = list.findIndex(
      (e) => e.projectId === activeProjectId && e.worktree.id === selectedWorktreeId,
    );
    setSelected(activeIdx > 0 ? activeIdx : 0);
    const handle = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(handle);
  }, [isOpen, activeProjectId, selectedWorktreeId]);

  // Any typed query resets the selection to the top match.
  useEffect(() => {
    if (query.trim()) setSelected(0);
  }, [query]);

  // Same activation as clicking a worktree in LeftSidebar: clear unseen
  // markers, switch active worktree/project, close.
  const activate = (e: Entry) => {
    const tStore = useTerminalStore.getState();
    tStore.sessions
      .filter((s) => s.worktreeId === e.worktree.id && s.hasUnseenActivity)
      .forEach((s) => tStore.markSessionSeen(s.id));
    void useProjectStore.getState().setActiveWorktree(e.projectId, e.worktree.id);
    onClose();
  };

  if (!isOpen) {
    return null;
  }

  const clamped = Math.min(selected, Math.max(filtered.length - 1, 0));

  return (
    <Dialog
      title="Search Worktrees"
      icon={<GitBranch size={16} className="text-[var(--color-mauve)]" />}
      width="560px"
      placement="top"
      onClose={onClose}
    >
      <input
        ref={inputRef}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setSelected((s) => Math.min(s + 1, filtered.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setSelected((s) => Math.max(s - 1, 0));
          } else if (e.key === "Enter" && filtered[clamped]) {
            activate(filtered[clamped]);
          } else if (e.key === "Escape") {
            onClose();
          }
        }}
        placeholder="Search by project, branch or name…"
        className="mb-2 w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-surface0)] px-3 py-2 text-sm text-[var(--color-text)] outline-none placeholder:text-[var(--color-overlay0)] focus:border-[var(--color-blue)]"
      />
      <div className="no-scrollbar max-h-80 overflow-y-auto">
        {filtered.length === 0 && (
          <p className="px-1 py-3 text-center text-xs text-[var(--color-overlay0)]">
            {query ? "No matching worktrees" : "No worktrees"}
          </p>
        )}
        {filtered.map((entry, i) => {
          const isActive =
            entry.projectId === activeProjectId &&
            entry.worktree.id === selectedWorktreeId;
          const activity = getWorktreeActivity(
            sessions,
            entry.projectId,
            entry.worktree.id,
          );
          const terminalCount = sessions.filter(
            (s) => s.projectId === entry.projectId && s.worktreeId === entry.worktree.id,
          ).length;
          const pr = prCache[`${entry.projectId}:${entry.worktree.branch}`]?.pr;
          return (
            <div
              key={`${entry.projectId}:${entry.worktree.id}`}
              className={`flex cursor-pointer items-start gap-2 rounded-md px-2 py-1 text-xs ${
                i === clamped
                  ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
                  : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50"
              }`}
              onClick={() => activate(entry)}
              onMouseEnter={() => setSelected(i)}
              title={`${entry.name} — ${entry.worktree.branch}${entry.projectName ? ` — ${entry.projectName}` : ""}${isActive ? " (active)" : ""}`}
            >
              <div className="flex w-[18px] shrink-0 justify-center pt-0.5">
                {activity === "busy" ? (
                  <Loader2 size={10} className="animate-spin text-[var(--color-blue)]" />
                ) : activity === "unseen" ? (
                  <span className="animate-blink text-[10px] font-bold text-[var(--color-green)]">
                    !
                  </span>
                ) : terminalCount > 0 ? (
                  <span
                    className={`text-[9px] leading-none ${
                      activity === "input"
                        ? "text-[var(--color-blue)]"
                        : "text-[var(--color-overlay1)]"
                    }`}
                  >
                    {terminalCount}
                  </span>
                ) : null}
              </div>
              <span className="min-w-0 flex-1">
                <span className="flex w-full items-center gap-2">
                  <span className="truncate">{entry.name}</span>
                  <span className="shrink-0 text-[10px] text-[var(--color-overlay1)]">
                    {entry.projectName}
                  </span>
                </span>
                <span className="flex w-full items-center gap-1.5 text-[10px] text-[var(--color-overlay1)]">
                  <GitBranch size={9} className="shrink-0" />
                  <span className="truncate">{entry.worktree.branch}</span>
                  {pr && <PrBadge pr={pr} />}
                </span>
              </span>
            </div>
          );
        })}
      </div>
    </Dialog>
  );
}
