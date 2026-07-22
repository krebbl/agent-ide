import { useCallback, useRef } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import { Pane } from "../../types";
import { useTerminalStore } from "../../stores/terminalStore";
import TerminalView from "./TerminalView";

interface SplitPaneContainerProps {
  pane: Pane;
  focusedPaneId: string;
  depth?: number;
  isWorktreeHidden?: boolean;
}

export default function SplitPaneContainer({
  pane,
  focusedPaneId,
  depth = 0,
  isWorktreeHidden = false,
}: SplitPaneContainerProps) {
  if (pane.type === "leaf") {
    return (
      <LeafPaneView
        pane={pane}
        isFocused={pane.id === focusedPaneId}
        isWorktreeHidden={isWorktreeHidden}
      />
    );
  }

  return (
    <SplitPaneView
      pane={pane}
      focusedPaneId={focusedPaneId}
      depth={depth}
      isWorktreeHidden={isWorktreeHidden}
    />
  );
}

function LeafPaneView({
  pane,
  isFocused,
  isWorktreeHidden,
}: {
  pane: { type: "leaf"; id: string; sessionId: string };
  isFocused: boolean;
  isWorktreeHidden: boolean;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const focusPane = useTerminalStore((s) => s.focusPane);
  const ptyId = useTerminalStore(
    (s) => s.sessions.find((session) => session.id === pane.sessionId)?.ptyId ?? "",
  );

  const handleClick = useCallback(() => {
    focusPane(pane.id);
  }, [focusPane, pane.id]);

  return (
    <div
      ref={containerRef}
      className={`relative h-full w-full ${isFocused ? "ring-1 ring-[var(--color-blue)]" : ""}`}
      onClick={handleClick}
    >
      <TerminalView
        sessionId={pane.sessionId}
        ptyId={ptyId}
        isFocused={isFocused}
        isCollapsed={isWorktreeHidden}
      />
    </div>
  );
}

function SplitPaneView({
  pane,
  focusedPaneId,
  depth,
  isWorktreeHidden,
}: {
  pane: { type: "split"; id: string; direction: "horizontal" | "vertical"; children: [Pane, Pane]; sizes: [number, number] };
  focusedPaneId: string;
  depth: number;
  isWorktreeHidden: boolean;
}) {
  const resizePane = useTerminalStore((s) => s.resizePane);

  const handleLayout = useCallback(
    (layout: { [id: string]: number }) => {
      const left = layout[`${pane.id}-0`];
      const right = layout[`${pane.id}-1`];
      if (left !== undefined && right !== undefined) {
        resizePane(pane.id, [left, right]);
      }
    },
    [resizePane, pane.id],
  );

  return (
    <Group
      orientation={pane.direction}
      className="h-full w-full"
      onLayoutChange={handleLayout}
    >
      <Panel id={`${pane.id}-0`} defaultSize={pane.sizes[0]} minSize={10}>
        <SplitPaneContainer
          pane={pane.children[0]}
          focusedPaneId={focusedPaneId}
          depth={depth + 1}
          isWorktreeHidden={isWorktreeHidden}
        />
      </Panel>
      <Separator
        className={`${
          pane.direction === "horizontal"
            ? "w-px cursor-col-resize"
            : "h-px cursor-row-resize"
        } bg-[var(--color-surface0)] transition-colors hover:bg-[var(--color-blue)]`}
      />
      <Panel id={`${pane.id}-1`} defaultSize={pane.sizes[1]} minSize={10}>
        <SplitPaneContainer
          pane={pane.children[1]}
          focusedPaneId={focusedPaneId}
          depth={depth + 1}
          isWorktreeHidden={isWorktreeHidden}
        />
      </Panel>
    </Group>
  );
}
