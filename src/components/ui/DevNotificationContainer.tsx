import { X } from "lucide-react";
import { useDevNotificationStore } from "../../stores/devNotificationStore";
import { useTerminalStore } from "../../stores/terminalStore";

export default function DevNotificationContainer() {
  const notifications = useDevNotificationStore((s) => s.notifications);
  const removeNotification = useDevNotificationStore((s) => s.removeNotification);
  const focusSession = useTerminalStore((s) => s.focusSession);

  if (notifications.length === 0) return null;

  return (
    <div className="pointer-events-none fixed right-4 top-12 z-50 flex max-w-xs flex-col gap-2">
      {notifications.map((notification) => (
        <div
          key={notification.id}
          onClick={() => {
            if (notification.sessionId) {
              focusSession(notification.sessionId);
            }
            removeNotification(notification.id);
          }}
          className={`pointer-events-auto flex items-start gap-2 rounded-md border bg-[var(--color-mantle)] py-2 pr-2 pl-3 shadow-lg transition-colors ${
            notification.sessionId
              ? "cursor-pointer border-[var(--color-surface0)] hover:border-[var(--color-blue)]"
              : "cursor-default border-[var(--color-surface0)]"
          }`}
        >
          <span className="mt-0.5 size-2 rounded-full bg-[var(--color-blue)]" />
          <span className="flex-1 text-xs text-[var(--color-text)]">
            {notification.message}
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              removeNotification(notification.id);
            }}
            className="text-[var(--color-overlay0)] transition-colors hover:text-[var(--color-text)]"
          >
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
