import type { TerminalSession } from "../stores/terminalStore";

export function getWorktreeActivity(
  sessions: TerminalSession[],
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
