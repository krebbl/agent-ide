import { Group, Panel, Separator } from "react-resizable-panels";
import TitleBar from "./TitleBar";
import StatusBar from "./StatusBar";
import LeftSidebar from "../sidebar/LeftSidebar";
import RightSidebar from "../sidebar/RightSidebar";
import MainArea from "../main/MainArea";

export default function AppLayout() {
  return (
    <div className="flex h-full w-full flex-col overflow-hidden bg-[var(--color-base)]">
      <TitleBar />
      <div className="flex flex-1 overflow-hidden">
        <Group orientation="horizontal" className="flex h-full w-full">
          <Panel
            defaultSize={20}
            minSize={10}
            maxSize={40}
            className="bg-[var(--color-mantle)]"
          >
            <LeftSidebar />
          </Panel>
          <Separator className="w-px bg-[var(--color-surface0)] transition-colors hover:bg-[var(--color-blue)]" />
          <Panel
            defaultSize={60}
            minSize={30}
            className="bg-[var(--color-base)]"
          >
            <MainArea />
          </Panel>
          <Separator className="w-px bg-[var(--color-surface0)] transition-colors hover:bg-[var(--color-blue)]" />
          <Panel
            defaultSize={20}
            minSize={10}
            maxSize={40}
            className="bg-[var(--color-mantle)]"
          >
            <RightSidebar />
          </Panel>
        </Group>
      </div>
      <StatusBar />
    </div>
  );
}
