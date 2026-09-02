import { GitMerge, GitPullRequest, GitPullRequestClosed, GitPullRequestDraft } from "lucide-react";
import { PrInfo } from "../../types";
import { openUrl } from "../../utils/openUrl";

const stateIcon = {
  open: GitPullRequest,
  merged: GitMerge,
  closed: GitPullRequestClosed,
  draft: GitPullRequestDraft,
} as const;

const stateColor: Record<PrInfo["state"], string> = {
  open: "text-[var(--color-green)]",
  merged: "text-[var(--color-mauve)]",
  closed: "text-[var(--color-red)]",
  draft: "text-[var(--color-overlay1)]",
};

function checkColor(pr: PrInfo): string {
  return pr.checkStatus === "success"
    ? "text-[var(--color-green)]"
    : pr.checkStatus === "failure"
      ? "text-[var(--color-red)]"
      : pr.checkStatus === "pending"
        ? "text-[var(--color-yellow)]"
        : "text-[var(--color-subtext1)]";
}

export default function PrBadge({ pr, className = "" }: { pr: PrInfo; className?: string }) {
  const Icon = stateIcon[pr.state];
  return (
    <span className={`flex shrink-0 items-center gap-1 ${className}`}>
      <Icon size={10} className={stateColor[pr.state]} />
      <button
        onClick={(e) => {
          e.stopPropagation();
          openUrl(pr.url).catch(() => {});
        }}
        className={`text-[10px] ${checkColor(pr)} hover:text-[var(--color-blue)] hover:underline`}
        title={pr.title}
      >
        #{pr.number}
      </button>
    </span>
  );
}
