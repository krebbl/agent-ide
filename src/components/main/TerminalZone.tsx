import {
  Terminal,
  Plus,
  X,
  ChevronDown,
  ChevronUp,
  Circle,
  Loader2,
} from "lucide-react";
import { useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTerminalStore } from "../../stores/terminalStore";
import { useProjectStore } from "../../stores/projectStore";
import TerminalView from "./TerminalView";

interface TerminalZoneProps {
  isCollapsed: boolean;
  onToggleCollapse: () => void;
}

export default function TerminalZone({
  isCollapsed,
  onToggleCollapse,
}: TerminalZoneProps) {
  const sessions = useTerminalStore((s) => s.sessions);
  const activeSessionId = useTerminalStore((s) => s.activeSessionId);
  const addSession = useTerminalStore((s) => s.addSession);
  const removeSession = useTerminalStore((s) => s.removeSession);
  const setActiveSession = useTerminalStore((s) => s.setActiveSession);

  const activeProjectId = useProjectStore((s) => s.activeProjectId);
  const selectedWorktreeId = useProjectStore((s) => s.selectedWorktreeId);

  const visibleSessions = useMemo(
    () =>
      sessions.filter(
        (s) =>
          s.projectId === activeProjectId && s.worktreeId === selectedWorktreeId,
      ),
    [sessions, activeProjectId, selectedWorktreeId],
  );

  const effectiveActiveId =
    visibleSessions.find((s) => s.id === activeSessionId)?.id ??
    visibleSessions[visibleSessions.length - 1]?.id ??
    null;

  useEffect(() => {
    if (isCollapsed) {
      invoke("pty_set_active", { ptyId: null }).catch(() => {});
      return;
    }
    const activeSession = visibleSessions.find((s) => s.id === effectiveActiveId);
    if (activeSession) {
      invoke("pty_set_active", { ptyId: activeSession.ptyId }).catch(() => {});
    } else {
      invoke("pty_set_active", { ptyId: null }).catch(() => {});
    }
  }, [effectiveActiveId, visibleSessions, isCollapsed]);

  const canAddTerminal = activeProjectId && selectedWorktreeId;

  const handleNewTerminal = () => {
    if (!canAddTerminal) return;
    addSession(undefined, undefined, activeProjectId, selectedWorktreeId).catch(
      () => {},
    );
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-[var(--color-surface0)] px-2">
        <Terminal size={14} className="text-[var(--color-green)]" />
        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
          Terminal
        </span>

        <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto px-1 no-scrollbar">
          {visibleSessions.map((session) => {
            const isActive = session.id === effectiveActiveId;
            return (
              <div
                key={session.id}
                onClick={() => setActiveSession(session.id)}
                className={`group flex shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2 py-1 text-xs transition-colors ${
                  isActive
                    ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
                    : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50 hover:text-[var(--color-text)]"
                }`}
                title={session.cwd}
              >
                {session.needsInput && (
                  <Circle
                    size={7}
                    className="fill-[var(--color-green)] text-[var(--color-green)]"
                    aria-label="Needs input"
                  />
                )}
                {session.isBusy && !session.needsInput && (
                  <Loader2
                    size={12}
                    className="animate-spin text-[var(--color-yellow)]"
                    aria-label="Busy"
                  />
                )}
                <span className="max-w-[120px] truncate">{session.title}</span>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    removeSession(session.id).catch(() => {});
                  }}
                  className="rounded-sm text-[var(--color-overlay0)] opacity-60 transition-colors hover:bg-[var(--color-surface1)] hover:text-[var(--color-text)] group-hover:opacity-100"
                >
                  <X size={12} />
                </button>
              </div>
            );
          })}
        </div>

        <button
          onClick={handleNewTerminal}
          disabled={!canAddTerminal}
          className="shrink-0 text-[var(--color-overlay1)] transition-colors hover:text-[var(--color-blue)] disabled:opacity-40 disabled:cursor-not-allowed"
          title="New terminal"
        >
          <Plus size={16} />
        </button>

        <button
          onClick={onToggleCollapse}
          className="shrink-0 text-[var(--color-overlay1)] transition-colors hover:text-[var(--color-text)]"
          title={isCollapsed ? "Expand terminal" : "Collapse terminal"}
        >
          {isCollapsed ? <ChevronDown size={16} /> : <ChevronUp size={16} />}
        </button>
      </div>

      <div className="relative flex-1 overflow-hidden bg-[var(--color-base)]">
        {visibleSessions.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-4">
            <span className="text-sm text-[var(--color-overlay0)]">
              {canAddTerminal
                ? "No terminal sessions for this worktree"
                : "Select a worktree to open a terminal"}
            </span>
            {canAddTerminal && (
              <button
                onClick={handleNewTerminal}
                className="flex items-center gap-1.5 rounded-md bg-[var(--color-surface0)] px-3 py-1.5 text-xs text-[var(--color-subtext0)] transition-colors hover:bg-[var(--color-surface1)] hover:text-[var(--color-text)]"
              >
                <Plus size={14} />
                New Terminal
              </button>
            )}
          </div>
        )}
        {sessions.map((session) => (
          <TerminalView
            key={session.id}
            sessionId={session.id}
            ptyId={session.ptyId}
            isActive={session.id === effectiveActiveId}
            isCollapsed={isCollapsed}
          />
        ))}
      </div>
    </div>
  );
}
