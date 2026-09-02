import { useEffect } from "react";
import { useEditorStore } from "../stores/editorStore";
import { findLeaf, useTerminalStore } from "../stores/terminalStore";
import { useUiStore } from "../stores/uiStore";

export function useCloseShortcut() {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const isMod = e.metaKey || e.ctrlKey;
      if (!isMod || e.shiftKey || e.key.toLowerCase() !== "w") return;

      const { focusedZone } = useUiStore.getState();

      if (focusedZone === "terminal") {
        const { tabs, activeTabId } = useTerminalStore.getState();
        const tab = tabs.find((t) => t.id === activeTabId);
        if (tab?.focusedPaneId) {
          const leaf = findLeaf(tab.rootPane, tab.focusedPaneId);
          if (leaf) {
            e.preventDefault();
            e.stopPropagation();
            void useTerminalStore.getState().removeSession(leaf.sessionId);
            return;
          }
        }
      }

      const { activePath, requestClose } = useEditorStore.getState();
      if (activePath) {
        e.preventDefault();
        e.stopPropagation();
        requestClose(activePath);
      }
    };

    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, []);
}
