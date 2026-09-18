import { useEffect } from "react";
import {
  Group,
  Panel,
  Separator,
  usePanelRef,
} from "react-resizable-panels";
import EditorZone from "./EditorZone";
import TerminalZone from "./TerminalZone";
import DiffPanel from "./DiffPanel";
import { useTerminalStore } from "../../stores/terminalStore";
import { useEditorStore } from "../../stores/editorStore";
import { useUiStore } from "../../stores/uiStore";

export default function MainArea() {
  const terminalPanelRef = usePanelRef();
  const editorPanelRef = usePanelRef();
  const diffPanelRef = usePanelRef();
  const isTerminalCollapsed = useTerminalStore((s) => s.isCollapsed);
  const setIsTerminalCollapsed = useTerminalStore((s) => s.setCollapsed);
  const diffPanelOpen = useUiStore((s) => s.diffPanelOpen);
  const hasOpenFiles = useEditorStore((s) => s.openFiles.length > 0);

  useEffect(() => {
    const panel = terminalPanelRef.current;
    if (!panel) return;
    if (isTerminalCollapsed && !panel.isCollapsed()) {
      panel.collapse();
    } else if (!isTerminalCollapsed && panel.isCollapsed()) {
      panel.expand();
    }
  }, [isTerminalCollapsed]);

  useEffect(() => {
    const panel = diffPanelRef.current;
    if (!panel) return;
    if (diffPanelOpen && panel.isCollapsed()) {
      panel.expand();
    } else if (!diffPanelOpen && !panel.isCollapsed()) {
      panel.collapse();
    }
  }, [diffPanelOpen]);

  useEffect(() => {
    const panel = editorPanelRef.current;
    if (!panel) return;
    if (hasOpenFiles && panel.isCollapsed()) {
      panel.expand();
    } else if (!hasOpenFiles && !panel.isCollapsed()) {
      panel.collapse();
    }
  }, [hasOpenFiles]);

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
      <Panel
        panelRef={editorPanelRef}
        defaultSize="60%"
        minSize="10%"
        collapsedSize={0}
        collapsible
        className="bg-[var(--color-base)]"
      >
        <EditorZone />
      </Panel>
      <Separator className="h-px bg-[var(--color-surface0)] transition-colors hover:bg-[var(--color-blue)]" />
      <Panel
        panelRef={diffPanelRef}
        defaultSize="35%"
        minSize="10%"
        collapsedSize={0}
        collapsible
        className="bg-[var(--color-base)]"
      >
        {diffPanelOpen ? <DiffPanel /> : null}
      </Panel>
    </Group>
  );
}