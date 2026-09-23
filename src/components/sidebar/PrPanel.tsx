import { useEffect } from "react";
import { CheckCircle2, ExternalLink, MessageSquare, RefreshCw } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import { usePrStore, PrThreadsEntry } from "../../stores/prStore";
import { useUiStore } from "../../stores/uiStore";
import { PrComment, PrInfo, PrReviewState } from "../../types";
import { openUrl } from "../../utils/openUrl";
import { prChangesUrl } from "../../utils/prUrl";
import { renderMarkdown } from "../../utils/renderMarkdown";

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

function stateColor(state: PrInfo["state"]): string {
  if (state === "merged") return "text-[var(--color-mauve)]";
  if (state === "closed") return "text-[var(--color-red)]";
  if (state === "draft") return "text-[var(--color-overlay1)]";
  return "text-[var(--color-green)]";
}

function ReviewChips({
  label,
  authors,
  color,
}: {
  label: string;
  authors: string[];
  color: string;
}) {
  if (authors.length === 0) return null;
  return (
    <div className="flex gap-2 text-xs">
      <span className="w-24 shrink-0 text-[var(--color-overlay1)]">{label}</span>
      <div className="flex flex-wrap gap-1">
        {authors.map((author) => (
          <span
            key={author}
            className={`rounded bg-[var(--color-surface0)] px-1.5 py-0.5 ${color}`}
          >
            {author}
          </span>
        ))}
      </div>
    </div>
  );
}

function CommentCard({ comment }: { comment: PrComment }) {
  return (
    <div className="rounded border border-[var(--color-surface0)] p-2">
      <div className="flex flex-wrap items-center gap-1.5 text-xs">
        <span className="font-medium text-[var(--color-subtext1)]">{comment.author}</span>
        <span className="text-[var(--color-overlay1)]">{formatDateTime(comment.createdAt)}</span>
        {comment.outdated && (
          <span className="rounded bg-[var(--color-surface1)] px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-[var(--color-yellow)]">
            Outdated
          </span>
        )}
        {comment.resolved && (
          <span className="rounded bg-[var(--color-surface1)] px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-[var(--color-green)]">
            Resolved
          </span>
        )}
        {comment.viewerReplied && (
          <span className="rounded bg-[var(--color-surface1)] px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-[var(--color-blue)]">
            You replied
          </span>
        )}
      </div>
      {comment.filePath && (
        <div className="mt-1 truncate font-mono text-[11px] text-[var(--color-overlay1)]" title={comment.filePath}>
          {comment.filePath}
        </div>
      )}
      <div
        className="rendered-markdown mt-1 break-words text-xs leading-relaxed text-[var(--color-text)]"
        dangerouslySetInnerHTML={{ __html: renderMarkdown(comment.body) }}
      />
    </div>
  );
}

function PrView({
  pr,
  onRefresh,
  refreshing,
  threads,
}: {
  pr: PrInfo;
  onRefresh: () => void;
  refreshing: boolean;
  threads: PrThreadsEntry | undefined;
}) {
  const changesUrl = prChangesUrl(pr);
  const byState = (state: PrReviewState) =>
    pr.reviews.filter((r) => r.state === state).map((r) => r.author);
  const approved = byState("approved");
  const changesRequested = byState("changes_requested");
  const missing = pr.reviewRequests.filter(
    (name) => !approved.includes(name) && !changesRequested.includes(name),
  );

  const comments = [
    ...(threads?.comments ?? []),
    ...pr.comments,
  ].sort((a, b) => b.createdAt.localeCompare(a.createdAt));
  const threadsError = threads?.error;
  const rateLimited = (msg: string | null | undefined) =>
    !!msg && /rate limit/i.test(msg);

  return (
    <div className="flex flex-col gap-3 p-3">
      <div className="sticky top-0 z-10 -mx-3 -mt-3 flex items-center gap-2 border-b border-[var(--color-surface0)] bg-[var(--color-base)] px-3 pb-2 pt-3">
        <button
          onClick={() => void openUrl(pr.url)}
          className="flex items-center gap-1.5 text-sm font-semibold text-[var(--color-blue)] hover:underline"
          title={pr.url}
        >
          PR #{pr.number}
          <ExternalLink size={12} />
        </button>
        <span className={`text-xs font-medium ${stateColor(pr.state)}`}>{pr.state}</span>
        <span className="rounded bg-[var(--color-surface0)] px-1.5 py-0.5 text-xs text-[var(--color-subtext0)]">
          {pr.provider}
        </span>
        <button
          onClick={onRefresh}
          disabled={refreshing}
          className="ml-auto text-[var(--color-overlay1)] transition-colors hover:text-[var(--color-blue)] disabled:opacity-40"
          title="Refresh"
        >
          <RefreshCw size={13} className={refreshing ? "animate-spin" : ""} />
        </button>
      </div>

      <div className="text-sm text-[var(--color-text)]">{pr.title}</div>

      <div className="flex flex-col gap-1">
        <div className="flex gap-2 text-xs">
          <span className="w-24 shrink-0 text-[var(--color-overlay1)]">Branches</span>
          <span className="text-[var(--color-subtext1)]">
            {pr.sourceBranch} → {pr.targetBranch}
          </span>
        </div>
        <div className="flex gap-2 text-xs">
          <span className="w-24 shrink-0 text-[var(--color-overlay1)]">Author</span>
          <span className="text-[var(--color-subtext1)]">{pr.author}</span>
        </div>
        <div className="flex gap-2 text-xs">
          <span className="w-24 shrink-0 text-[var(--color-overlay1)]">Updated</span>
          <span className="text-[var(--color-subtext1)]">{formatDateTime(pr.updatedAt)}</span>
        </div>
      </div>

      <div className="flex items-center gap-3 text-xs">
        <button
          onClick={() => void openUrl(pr.url)}
          className="flex items-center gap-1.5 rounded border border-[var(--color-surface0)] px-2 py-1 text-[var(--color-subtext1)] transition-colors hover:border-[var(--color-blue)] hover:text-[var(--color-blue)]"
        >
          <ExternalLink size={12} />
          View PR
        </button>
        {changesUrl && (
          <button
            onClick={() => void openUrl(changesUrl)}
            className="flex items-center gap-1.5 rounded border border-[var(--color-surface0)] px-2 py-1 text-[var(--color-subtext1)] transition-colors hover:border-[var(--color-blue)] hover:text-[var(--color-blue)]"
          >
            <MessageSquare size={12} />
            File changes
          </button>
        )}
      </div>

      <div className="flex flex-col gap-1 border-t border-[var(--color-surface0)] pt-2">
        <div className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
          <CheckCircle2 size={12} />
          Approvals
        </div>
        {pr.reviews.length === 0 && pr.reviewRequests.length === 0 && (
          <div className="text-xs text-[var(--color-overlay1)]">
            No reviews yet and no reviewers requested
          </div>
        )}
        <ReviewChips
          label="Approved"
          authors={approved}
          color="text-[var(--color-green)]"
        />
        <ReviewChips
          label="Changes requested"
          authors={changesRequested}
          color="text-[var(--color-red)]"
        />
        <ReviewChips label="Missing" authors={missing} color="text-[var(--color-yellow)]" />
        <ReviewChips
          label="Commented"
          authors={byState("commented")}
          color="text-[var(--color-subtext0)]"
        />
      </div>

      <div className="flex items-center gap-1.5 border-t border-[var(--color-surface0)] pt-2 text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
        Comments ({comments.length})
      </div>
      {threadsError && !rateLimited(threadsError) && (
        <div className="text-xs text-[var(--color-peach)]">{threadsError}</div>
      )}
      {rateLimited(threadsError) && (
        <div className="text-xs text-[var(--color-yellow)]">
          GitHub API rate limit exceeded — review comments are temporarily unavailable. Approvals
          above come from cached data; try again later.
        </div>
      )}
      <div className="flex flex-col gap-2">
        {threads?.loading && comments.length === 0 && (
          <div className="text-xs text-[var(--color-overlay1)]">Loading comments…</div>
        )}
        {!threads?.loading && comments.length === 0 && !threadsError && (
          <div className="text-xs text-[var(--color-overlay1)]">
            {pr.provider === "bitbucket"
              ? "Comment threads are not available for Bitbucket PRs yet"
              : "No comments"}
          </div>
        )}
        {comments.map((comment, i) => (
          <CommentCard key={comment.id || i} comment={comment} />
        ))}
      </div>
    </div>
  );
}

export default function PrPanel() {
  const { projects, activeProjectId } = useProjectStore();
  const fetchPrForBranch = usePrStore((s) => s.fetchPrForBranch);
  const fetchThreads = usePrStore((s) => s.fetchThreads);
  const isActiveTab = useUiStore((s) => s.rightSidebarTab === "pr");

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

  const entry = usePrStore((s) =>
    projectId && branch ? s.cache[`${projectId}:${branch}`] : undefined,
  );
  const pr = entry?.pr ?? null;
  const threadsKey = projectId && branch ? `${projectId}:${branch}` : null;
  const threads = usePrStore((s) => (threadsKey ? s.threads[threadsKey] : undefined));

  useEffect(() => {
    if (!isActiveTab || !projectId || !pr) return;
    void fetchThreads(projectId, branch!, pr.number);
  }, [isActiveTab, projectId, branch, pr?.number, fetchThreads]);

  if (!projectId || !activeProject) {
    return <div className="p-3 text-xs text-[var(--color-overlay1)]">No project selected</div>;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex-1 overflow-y-auto">
        {entry?.loading && !pr && (
          <div className="p-3 text-xs text-[var(--color-overlay1)]">Loading pull request…</div>
        )}
        {!entry?.loading && !pr && (
          <div className="p-3 text-xs text-[var(--color-overlay1)]">
            {entry?.error ?? "No pull request found for this branch"}
          </div>
        )}
        {pr && (
          <PrView
            pr={pr}
            refreshing={!!entry?.loading || !!threads?.loading}
            threads={threads}
            onRefresh={() => {
              if (!branch) return;
              void fetchPrForBranch(projectId, branch, true);
              void fetchThreads(projectId, branch, pr.number, true);
            }}
          />
        )}
      </div>
    </div>
  );
}
