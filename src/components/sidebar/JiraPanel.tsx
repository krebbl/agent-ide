import { useEffect } from "react";
import { ExternalLink, RefreshCw, Settings } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import { useJiraStore } from "../../stores/jiraStore";
import { useUiStore } from "../../stores/uiStore";
import { JiraIssue } from "../../types";
import { openUrl } from "../../utils/openUrl";

function formatDateTime(iso: string): string {
  if (!iso) return "";
  return new Date(iso).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function statusColor(status: string): string {
  const s = status.toLowerCase();
  if (s === "done" || s === "resolved" || s === "closed") return "text-[var(--color-green)]";
  if (s.includes("progress") || s === "in review" || s === "review") return "text-[var(--color-yellow)]";
  if (s === "to do" || s === "open" || s === "backlog") return "text-[var(--color-overlay1)]";
  return "text-[var(--color-blue)]";
}

function MetaRow({ label, value }: { label: string; value: string | null }) {
  if (!value) return null;
  return (
    <div className="flex gap-2 text-xs">
      <span className="w-20 shrink-0 text-[var(--color-overlay1)]">{label}</span>
      <span className="text-[var(--color-text)]">{value}</span>
    </div>
  );
}

function IssueView({ issue }: { issue: JiraIssue }) {
  return (
    <div className="flex flex-col gap-3 p-3">
      <div className="flex items-center gap-2">
        <button
          onClick={() => void openUrl(issue.url)}
          className="flex items-center gap-1.5 text-sm font-semibold text-[var(--color-blue)] hover:underline"
          title={issue.url}
        >
          {issue.key}
          <ExternalLink size={12} />
        </button>
        <span className={`text-xs font-medium ${statusColor(issue.status)}`}>{issue.status}</span>
        <span className="rounded bg-[var(--color-surface0)] px-1.5 py-0.5 text-xs text-[var(--color-subtext0)]">
          {issue.issueType}
        </span>
      </div>
      <div className="text-sm text-[var(--color-text)]">{issue.summary}</div>
      <div className="flex flex-col gap-1">
        <MetaRow label="Priority" value={issue.priority} />
        <MetaRow label="Assignee" value={issue.assignee} />
        <MetaRow label="Reporter" value={issue.reporter} />
        {issue.labels.length > 0 && (
          <div className="flex gap-2 text-xs">
            <span className="w-20 shrink-0 text-[var(--color-overlay1)]">Labels</span>
            <div className="flex flex-wrap gap-1">
              {issue.labels.map((label) => (
                <span key={label} className="rounded bg-[var(--color-surface0)] px-1.5 py-0.5 text-[var(--color-subtext0)]">
                  {label}
                </span>
              ))}
            </div>
          </div>
        )}
        <MetaRow label="Created" value={formatDateTime(issue.created)} />
        <MetaRow label="Updated" value={formatDateTime(issue.updated)} />
      </div>
      {issue.description && (
        <div className="whitespace-pre-wrap break-words rounded border border-[var(--color-surface0)] bg-[var(--color-mantle)] p-2 text-xs leading-relaxed text-[var(--color-subtext1)]">
          {issue.description}
        </div>
      )}
      <div className="flex items-center gap-1.5 border-t border-[var(--color-surface0)] pt-2 text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
        Comments ({issue.comments.length})
      </div>
      <div className="flex flex-col gap-2">
        {issue.comments.length === 0 && (
          <div className="text-xs text-[var(--color-overlay1)]">No comments</div>
        )}
        {issue.comments.map((comment, i) => (
          <div key={i} className="rounded border border-[var(--color-surface0)] p-2">
            <div className="flex items-center justify-between text-xs">
              <span className="font-medium text-[var(--color-subtext1)]">{comment.author}</span>
              <span className="text-[var(--color-overlay1)]">{formatDateTime(comment.created)}</span>
            </div>
            <div className="mt-1 whitespace-pre-wrap break-words text-xs leading-relaxed text-[var(--color-text)]">
              {comment.body}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function JiraPanel() {
  const { projects, activeProjectId } = useProjectStore();
  const loadConfig = useJiraStore((s) => s.loadConfig);
  const fetchIssueForBranch = useJiraStore((s) => s.fetchIssueForBranch);
  const openProjectSettings = useUiStore((s) => s.openProjectSettings);

  const activeProject = activeProjectId
    ? projects.find((p) => p.id === activeProjectId)
    : projects.find((p) => p.worktrees.length > 0);
  const worktree = activeProject
    ? activeProject.activeWorktreeId
      ? activeProject.worktrees.find((w) => w.id === activeProject.activeWorktreeId)
      : activeProject.worktrees.find((w) => w.isMain)
    : undefined;
  const projectId = activeProject?.id;
  const branch = worktree?.branch;

  const config = useJiraStore((s) => (projectId ? s.configs[projectId] : undefined));
  const entry = useJiraStore((s) =>
    projectId && branch ? s.cache[`${projectId}:${branch}`] : undefined,
  );

  useEffect(() => {
    if (!projectId) return;
    void loadConfig(projectId);
  }, [projectId, loadConfig]);

  useEffect(() => {
    if (!config?.hasToken || !projectId || !branch) return;
    void fetchIssueForBranch(projectId, branch);
  }, [config?.hasToken, config?.siteUrl, config?.email, projectId, branch, fetchIssueForBranch]);

  if (!projectId || !activeProject) {
    return (
      <div className="p-3 text-xs text-[var(--color-overlay1)]">No project selected</div>
    );
  }

  const result = entry?.result;

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-8 shrink-0 items-center justify-between border-b border-[var(--color-surface0)] px-3">
        <span className="truncate text-xs text-[var(--color-overlay1)]" title={branch}>
          {branch ?? "No worktree"}
        </span>
        {config?.hasToken && (
          <button
            onClick={() => branch && void fetchIssueForBranch(projectId, branch, true)}
            disabled={!branch || entry?.loading}
            className="text-[var(--color-overlay1)] transition-colors hover:text-[var(--color-blue)] disabled:opacity-40"
            title="Refresh"
          >
            <RefreshCw size={13} className={entry?.loading ? "animate-spin" : ""} />
          </button>
        )}
      </div>
      <div className="flex-1 overflow-y-auto">
        {!config?.hasToken ? (
          <div className="flex flex-col items-start gap-2 p-3">
            <div className="text-xs text-[var(--color-overlay1)]">
              Jira is not configured for this project. Set the site URL, account email and an API
              token to show the ticket for the active branch.
            </div>
            <button
              onClick={() => openProjectSettings(projectId)}
              className="flex items-center gap-1.5 rounded border border-[var(--color-surface0)] px-2 py-1 text-xs text-[var(--color-subtext1)] transition-colors hover:border-[var(--color-blue)] hover:text-[var(--color-blue)]"
            >
              <Settings size={12} />
              Configure Jira
            </button>
          </div>
        ) : (
          <>
            {entry?.loading && !result && (
              <div className="p-3 text-xs text-[var(--color-overlay1)]">Loading ticket…</div>
            )}
            {result?.error && (
              <div className="p-3 text-xs text-[var(--color-peach)]">{result.error}</div>
            )}
            {result?.issue && <IssueView issue={result.issue} />}
          </>
        )}
      </div>
    </div>
  );
}
