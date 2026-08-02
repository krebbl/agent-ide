import { PanelBottom } from "lucide-react";
import { useLspStore, type LspServerStatus } from "../../stores/lspStore";
import { useTerminalStore } from "../../stores/terminalStore";
import { restartServer } from "../../services/lsp/coordinator";

const STATUS_COLORS: Record<LspServerStatus, string> = {
  ready: "var(--color-green)",
  starting: "var(--color-yellow)",
  crashed: "var(--color-red)",
  stopped: "var(--color-overlay1)",
  unavailable: "var(--color-overlay0)",
};

export default function StatusBar() {
  const servers = useLspStore((s) => s.servers);
  const entries = Object.entries(servers);
  const isCollapsed = useTerminalStore((s) => s.isCollapsed);
  const setCollapsed = useTerminalStore((s) => s.setCollapsed);

  return (
    <div className="flex h-6 shrink-0 items-center border-t border-[var(--color-surface0)] bg-[var(--color-crust)] px-3 text-xs text-[var(--color-subtext0)]">
      <span>Ready</span>
      <div className="ml-auto flex items-center gap-3">
        <button
          className={`transition-colors ${
            isCollapsed
              ? "text-[var(--color-overlay1)]"
              : "text-[var(--color-blue)]"
          } hover:text-[var(--color-blue)]`}
          title={isCollapsed ? "Show panel" : "Hide panel"}
          onClick={() => setCollapsed(!isCollapsed)}
        >
          <PanelBottom size={13} />
        </button>
        {entries.map(([key, { status, error }]) => {
          const serverKey = key.slice(key.lastIndexOf(":") + 1);
          const projectId = key.slice(0, key.lastIndexOf(":"));
          const canRestart =
            status === "crashed" ||
            status === "stopped" ||
            status === "unavailable";
          return (
            <button
              key={key}
              className={`flex items-center gap-1.5 ${canRestart ? "hover:text-[var(--color-text)]" : "cursor-default"}`}
              title={
                error ??
                `${serverKey}: ${status}${canRestart ? " (click to restart)" : ""}`
              }
              onClick={() => {
                if (canRestart) void restartServer(projectId, serverKey);
              }}
            >
              <span
                className="h-2 w-2 rounded-full"
                style={{ backgroundColor: STATUS_COLORS[status] }}
              />
              {serverKey}
            </button>
          );
        })}
      </div>
    </div>
  );
}
