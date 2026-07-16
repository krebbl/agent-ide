import { Group, Panel, Separator } from "react-resizable-panels";
import TerminalZone from "./TerminalZone";
import EditorZone from "./EditorZone";

export default function MainArea() {
  return (
    <Group orientation="vertical" className="flex h-full w-full">
      <Panel
        defaultSize={40}
        minSize={10}
        className="bg-[var(--color-base)]"
      >
        <TerminalZone />
      </Panel>
      <Separator className="h-px bg-[var(--color-surface0)] transition-colors hover:bg-[var(--color-blue)]" />
      <Panel
        defaultSize={60}
        minSize={10}
        className="bg-[var(--color-base)]"
      >
        <EditorZone />
      </Panel>
    </Group>
  );
}
