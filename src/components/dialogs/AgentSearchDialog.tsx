import { useEffect, useMemo, useRef, useState } from "react";
import { Bot } from "lucide-react";
import Dialog from "../ui/Dialog";
import { TerminalSession, useTerminalStore } from "../../stores/terminalStore";
import { useProjectStore } from "../../stores/projectStore";

interface AgentSearchDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function AgentSearchDialog({
  isOpen,
  onClose,
}: AgentSearchDialogProps) {
  const sessions = useTerminalStore((s) => s.sessions);
  const projects = useProjectStore((s) => s.projects);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const projectNameById = useMemo(
    () => Object.fromEntries(projects.map((p) => [p.id, p.name])),
    [projects],
  );
  const branchById = useMemo(
    () =>
      Object.fromEntries(
        projects.flatMap((p) =>
          p.worktrees.map((w) => [`${p.id}:${w.id}`, w.branch]),
        ),
      ) as Record<string, string>,
    [projects],
  );

  // Same visibility rule as the Active section in LeftSidebar: a session
  // counts as an active agent session when an agent was started in it and it
  // is still alive (running, busy, or waiting for input). Most recently used
  // first, so the default selection ("previous session") is the entry after
  // the currently focused one.
  const agentSessions = useMemo(
    () =>
      sessions
        .filter(
          (s) =>
            Boolean(s.projectId) &&
            s.agentName != null &&
            (s.agentActive ||
              s.isBusy === true ||
              s.processRunning === true ||
              s.needsInput),
        )
        .sort(
          (a, b) =>
            (b.lastActiveAt ?? b.createdAt ?? 0) -
            (a.lastActiveAt ?? a.createdAt ?? 0),
        ),
    [sessions],
  );

  const filtered = useMemo(() => {
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    if (terms.length === 0) return agentSessions;
    return agentSessions.filter((s) => {
      const fields = [
        s.title,
        projectNameById[s.projectId ?? ""] ?? "",
        branchById[`${s.projectId}:${s.worktreeId}`] ?? "",
      ].map((f) => f.toLowerCase());
      return terms.every((t) => fields.some((f) => f.includes(t)));
    });
  }, [agentSessions, query, projectNameById, branchById]);

  // Reset only when the dialog opens: query cleared, previous agent session
  // preselected (the one used before the currently focused agent session).
  // Deliberately keyed on `isOpen` alone — `agentSessions` changes identity
  // on every busy-state update, and resetting then would clear the query
  // while the user types. The list is read through a ref.
  const agentSessionsRef = useRef(agentSessions);
  agentSessionsRef.current = agentSessions;
  useEffect(() => {
    if (!isOpen) return;
    setQuery("");
    const activeSessionId = useTerminalStore.getState().activeSessionId;
    const list = agentSessionsRef.current;
    const activeIdx = list.findIndex((s) => s.id === activeSessionId);
    setSelected(list.length > 1 && activeIdx === 0 ? 1 : 0);
    const handle = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(handle);
  }, [isOpen]);

  // Any typed query resets the selection to the most recent match.
  useEffect(() => {
    if (query.trim()) setSelected(0);
  }, [query]);

  const jumpTo = (session: TerminalSession) => {
    const tStore = useTerminalStore.getState();
    if (session.hasUnseenActivity) {
      tStore.markSessionSeen(session.id);
    }
    tStore.focusSession(session.id);
    onClose();
  };

  if (!isOpen) {
    return null;
  }

  const clamped = Math.min(selected, Math.max(filtered.length - 1, 0));

  return (
    <Dialog
      title="Search Agent Sessions"
      icon={<Bot size={16} className="text-[var(--color-mauve)]" />}
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
            jumpTo(filtered[clamped]);
          } else if (e.key === "Escape") {
            onClose();
          }
        }}
        placeholder="Search by project, branch or title…"
        className="mb-2 w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-surface0)] px-3 py-2 text-sm text-[var(--color-text)] outline-none placeholder:text-[var(--color-overlay0)] focus:border-[var(--color-blue)]"
      />
      <div className="no-scrollbar max-h-80 overflow-y-auto">
        {filtered.length === 0 && (
          <p className="px-1 py-3 text-center text-xs text-[var(--color-overlay0)]">
            {query ? "No matching agent sessions" : "No active agent sessions"}
          </p>
        )}
        {filtered.map((session, i) => {
          const projectName = projectNameById[session.projectId ?? ""] ?? "";
          const branch =
            branchById[`${session.projectId}:${session.worktreeId}`] ?? "";
          const isBusy = session.isBusy || session.processRunning;
          return (
            <div
              key={session.id}
              className={`flex cursor-pointer items-start gap-2 rounded-md px-2 py-1 text-xs ${
                i === clamped
                  ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
                  : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50"
              }`}
              onClick={() => jumpTo(session)}
              onMouseEnter={() => setSelected(i)}
              title={`${session.agentName ?? "agent"} — ${session.title}${projectName ? ` — ${projectName}` : ""}${isBusy ? " (busy)" : ""}`}
            >
              <Bot
                size={10}
                className={`mt-0.5 shrink-0 ${
                  isBusy
                    ? "animate-blink text-[var(--color-green)]"
                    : session.hasUnseenActivity
                      ? "text-[var(--color-green)]"
                      : "text-[var(--color-mauve)]"
                }`}
              />
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
            </div>
          );
        })}
      </div>
    </Dialog>
  );
}
