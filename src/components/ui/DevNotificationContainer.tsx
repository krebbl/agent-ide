import { X } from "lucide-react";
import { useDevNotificationStore } from "../../stores/devNotificationStore";

export default function DevNotificationContainer() {
  const notifications = useDevNotificationStore((s) => s.notifications);
  const removeNotification = useDevNotificationStore((s) => s.removeNotification);

  if (notifications.length === 0) return null;

  return (
    <div className="pointer-events-none fixed right-4 top-12 z-50 flex max-w-xs flex-col gap-2">
      {notifications.map((notification) => (
        <div
          key={notification.id}
          className="pointer-events-auto flex items-start gap-2 rounded-md border border-[var(--color-surface0)] bg-[var(--color-mantle)] py-2 pr-2 pl-3 shadow-lg"
        >
          <span className="mt-0.5 size-2 rounded-full bg-[var(--color-blue)]" />
          <span className="flex-1 text-xs text-[var(--color-text)]">
            {notification.message}
          </span>
          <button
            onClick={() => removeNotification(notification.id)}
            className="text-[var(--color-overlay0)] transition-colors hover:text-[var(--color-text)]"
          >
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
