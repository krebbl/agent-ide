import { useCallback, useEffect, useState } from "react";
import { Bot, Loader2 } from "lucide-react";
import { AgentConversationSummary } from "../../types";
import { buildAgentResumeCommand, listRecentAgentConversations } from "../../services/agents";
import { useAgentStore } from "../../stores/agentStore";
import { useTerminalStore } from "../../stores/terminalStore";

interface RecentAgentSessionsProps {
  worktreePath: string;
  projectId: string;
  worktreeId: string;
}

function relativeTime(ts: number): string {
  const elapsed = Date.now() - ts;
  if (elapsed < 60_000) return "just now";
  const minutes = Math.floor(elapsed / 60_000);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(ts).toLocaleDateString();
}

export default function RecentAgentSessions({
  worktreePath,
  projectId,
  worktreeId,
}: RecentAgentSessionsProps) {
  const [sessions, setSessions] = useState<AgentConversationSummary[] | null>(null);
  const [launchingId, setLaunchingId] = useState<string | null>(null);
  const agents = useAgentStore((s) => s.agents);

  useEffect(() => {
    setSessions(null);
    let cancelled = false;
    listRecentAgentConversations(worktreePath)
      .then((entries) => {
        if (!cancelled) setSessions(entries.slice(0, 6));
      })
      .catch(() => {
        if (!cancelled) setSessions([]);
      });
    return () => {
      cancelled = true;
    };
  }, [worktreePath]);

  const handleSelect = useCallback(
    async (session: AgentConversationSummary) => {
      setLaunchingId(session.conversationId);
      try {
        const argv = await buildAgentResumeCommand(session.agent, session.conversationId);
        await useTerminalStore.getState().addSession(
          worktreePath,
          "local",
          projectId,
          worktreeId,
          argv,
        );
      } catch {
        // Backend already filtered to installed agents; a failure here just
        // means the spawn didn't happen.
      } finally {
        setLaunchingId(null);
      }
    },
    [worktreePath, projectId, worktreeId],
  );

  if (sessions === null || sessions.length === 0) return null;

  return (
    <div className="flex w-full max-w-md flex-col gap-1">
      <span className="mb-1 text-center text-xs font-semibold uppercase tracking-wide text-[var(--color-overlay0)]">
        Recent agent sessions
      </span>
      {sessions.map((session) => {
        const label = agents.find((a) => a.id === session.agent)?.label ?? session.agent;
        const launching = launchingId === session.conversationId;
        return (
          <button
            key={session.conversationId}
            onClick={() => void handleSelect(session)}
            className="flex w-full items-center gap-2.5 rounded-md bg-[var(--color-surface0)] px-3 py-2 text-left text-xs transition-colors hover:bg-[var(--color-surface1)]"
          >
            <Bot size={14} className="shrink-0 text-[var(--color-mauve)]" />
            <span className="min-w-0 flex-1 truncate text-[var(--color-text)]">{label}</span>
            <span
              title={session.conversationId}
              className="shrink-0 font-mono text-[var(--color-overlay0)]"
            >
              {session.conversationId.slice(0, 8)}
            </span>
            <span className="w-16 shrink-0 text-right text-[var(--color-overlay0)]">
              {relativeTime(session.lastActiveAt)}
            </span>
            {launching && (
              <Loader2 size={12} className="shrink-0 animate-spin text-[var(--color-blue)]" />
            )}
          </button>
        );
      })}
    </div>
  );
}
