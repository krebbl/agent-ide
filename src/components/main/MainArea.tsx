import { useEffect } from "react";
import {
  Group,
  Panel,
  Separator,
  usePanelRef,
} from "react-resizable-panels";
import TerminalZone from "./TerminalZone";
import EditorZone from "./EditorZone";
import { useTerminalStore } from "../../stores/terminalStore";

export default function MainArea() {
  const terminalPanelRef = usePanelRef();
  const isTerminalCollapsed = useTerminalStore((s) => s.isCollapsed);
  const setIsTerminalCollapsed = useTerminalStore((s) => s.setCollapsed);

  useEffect(() => {
    const panel = terminalPanelRef.current;
    if (!panel) return;
    if (isTerminalCollapsed && !panel.isCollapsed()) {
      panel.collapse();
    } else if (!isTerminalCollapsed && panel.isCollapsed()) {
      panel.expand();
    }
  }, [isTerminalCollapsed]);

  const handleToggleCollapse = () => {
    const panel = terminalPanelRef.current;
    if (!panel) return;
    if (panel.isCollapsed()) {
      panel.expand();
    } else {
      panel.collapse();
    }
  };

  return (
    <Group orientation="vertical" className="flex h-full w-full">
      <Panel
        panelRef={terminalPanelRef}
        defaultSize="40%"
        minSize="10%"
        collapsedSize={0}
        collapsible
        className="bg-[var(--color-base)]"
        onResize={() =>
          setIsTerminalCollapsed(terminalPanelRef.current?.isCollapsed() ?? false)
        }
      >
        <TerminalZone
          isCollapsed={isTerminalCollapsed}
          onToggleCollapse={handleToggleCollapse}
        />
      </Panel>
      <Separator className="h-px bg-[var(--color-surface0)] transition-colors hover:bg-[var(--color-blue)]" />
      <Panel defaultSize="60%" minSize="10%" className="bg-[var(--color-base)]">
        <EditorZone />
      </Panel>
    </Group>
  );
}
