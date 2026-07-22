import { useEffect, type ReactNode } from "react";
import { X } from "lucide-react";

interface DialogProps {
  title: string;
  icon?: ReactNode;
  width?: string;
  scrollable?: boolean;
  danger?: boolean;
  closeOnBackdrop?: boolean;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
}

export default function Dialog({
  title,
  icon,
  width = "400px",
  scrollable = false,
  danger = false,
  closeOnBackdrop = true,
  onClose,
  children,
  footer,
}: DialogProps) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const borderStyle = danger
    ? "1px solid color-mix(in srgb, var(--color-red) 30%, transparent)"
    : "1px solid var(--color-surface0)";

  const titleColor = danger ? "var(--color-red)" : "var(--color-text)";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={closeOnBackdrop ? onClose : undefined}
    >
      <div
        className={`flex flex-col rounded-lg bg-[var(--color-mantle)] shadow-2xl ${scrollable ? "max-h-[80vh]" : ""}`}
        style={{ width, border: borderStyle }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className={`flex items-center border-b border-[var(--color-surface0)] px-4 py-3 ${
            icon ? "gap-2" : "justify-between"
          }`}
        >
          {icon}
          <h2 className="text-sm font-semibold" style={{ color: titleColor }}>
            {title}
          </h2>
          <button
            onClick={onClose}
            className={`text-[var(--color-overlay1)] hover:text-[var(--color-text)] ${
              icon ? "ml-auto rounded p-0.5 hover:bg-[var(--color-surface0)]" : ""
            }`}
          >
            <X size={icon ? 16 : 18} />
          </button>
        </div>

        <div className={scrollable ? "flex-1 overflow-y-auto p-4" : "p-4"}>
          {children}
        </div>

        {footer && (
          <div className="flex justify-end gap-2 border-t border-[var(--color-surface0)] px-4 py-3">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
