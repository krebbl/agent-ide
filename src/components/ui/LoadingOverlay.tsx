import { Loader2 } from "lucide-react";

export default function LoadingOverlay({ label }: { label?: string }) {
  return (
    <div className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-2 bg-[var(--color-base)]/80">
      <Loader2 size={20} className="animate-spin text-[var(--color-blue)]" />
      {label && (
        <span className="text-xs text-[var(--color-subtext0)]">{label}</span>
      )}
    </div>
  );
}
