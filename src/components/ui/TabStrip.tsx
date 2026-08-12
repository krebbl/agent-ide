import { X } from "lucide-react";
import type { ReactNode } from "react";

export interface TabItem {
  id: string;
  title: string;
  icon?: ReactNode;
  badge?: ReactNode;
  tooltip?: string;
}

interface TabStripProps {
  tabs: TabItem[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
}

export default function TabStrip({
  tabs,
  activeId,
  onSelect,
  onClose,
}: TabStripProps) {
  return (
    <div className="no-scrollbar flex min-w-0 flex-1 items-center gap-1 overflow-x-auto px-1">
      {tabs.map((tab) => {
        const isActive = tab.id === activeId;
        return (
          <div
            key={tab.id}
            onClick={() => onSelect(tab.id)}
            className={`group flex shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2 py-1 text-xs transition-colors ${
              isActive
                ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
                : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/50 hover:text-[var(--color-text)]"
            }`}
            title={tab.tooltip ?? tab.title}
          >
            {tab.icon}
            <span className="max-w-[120px] truncate select-none">
              {tab.title}
            </span>
            {tab.badge}
            <button
              onClick={(e) => {
                e.stopPropagation();
                onClose(tab.id);
              }}
              className="rounded-sm text-[var(--color-overlay0)] opacity-60 transition-colors hover:bg-[var(--color-surface1)] hover:text-[var(--color-text)] group-hover:opacity-100"
            >
              <X size={12} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
