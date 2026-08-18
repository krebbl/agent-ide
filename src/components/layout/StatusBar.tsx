import { Info, PanelBottom, PanelRight } from "lucide-react";
import { useLspStore, type LspServerStatus } from "../../stores/lspStore";
import { useTerminalStore } from "../../stores/terminalStore";
import { useProjectStore } from "../../stores/projectStore";
import { useEditorStore, languageFromPath } from "../../stores/editorStore";
import { useUiStore } from "../../stores/uiStore";
import { restartServer, serverKeyForPath } from "../../services/lsp/coordinator";

const STATUS_COLORS: Record<LspServerStatus, string> = {
  ready: "var(--color-green)",
  starting: "var(--color-yellow)",
  crashed: "var(--color-red)",
  stopped: "var(--color-overlay1)",
  unavailable: "var(--color-overlay0)",
};

export default function StatusBar() {
  const servers = useLspStore((s) => s.servers);
  const activeProjectId = useProjectStore((s) => s.activeProjectId);
  const entries = Object.entries(servers).filter(([key]) =>
    activeProjectId ? key.startsWith(activeProjectId + ":") : true,
  );
  const isCollapsed = useTerminalStore((s) => s.isCollapsed);
  const setCollapsed = useTerminalStore((s) => s.setCollapsed);
  const activePath = useEditorStore((s) => s.activePath);
  const activeLanguage = activePath ? languageFromPath(activePath) : null;
  const noLspForActive =
    activePath !== null && serverKeyForPath(activePath) === null;
  const { rightSidebarVisible, toggleRightSidebar } = useUiStore();

  return (
    <div className="flex h-6 shrink-0 items-center border-t border-[var(--color-surface0)] bg-[var(--color-crust)] px-3 text-xs text-[var(--color-subtext0)]">
      <span>Ready</span>
      <div className="ml-auto flex items-center gap-3">
        <button
          className={`transition-colors ${
            rightSidebarVisible
              ? "text-[var(--color-blue)]"
              : "text-[var(--color-overlay1)]"
          } hover:text-[var(--color-blue)]`}
          title={rightSidebarVisible ? "Hide side panel" : "Show side panel"}
          onClick={() => toggleRightSidebar()}
        >
          <PanelRight size={13} />
        </button>
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
        {noLspForActive && (
          <span
            className="flex items-center gap-1.5 text-[var(--color-overlay0)]"
            title={`No language server available for ${activeLanguage}`}
          >
            <Info size={11} />
            {activeLanguage}: no LSP
          </span>
        )}
      </div>
    </div>
  );
}
