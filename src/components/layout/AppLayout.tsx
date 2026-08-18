import { useEffect, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import StatusBar from "./StatusBar";
import LeftSidebar from "../sidebar/LeftSidebar";
import RightSidebar from "../sidebar/RightSidebar";
import MainArea from "../main/MainArea";
import DevNotificationContainer from "../ui/DevNotificationContainer";
import FileSearchDialog from "../dialogs/FileSearchDialog";
import { useUiStore } from "../../stores/uiStore";
import { useFileTreeStore } from "../../stores/fileTreeStore";

export default function AppLayout() {
  const { rightSidebarVisible } = useUiStore();
  const [fileSearchOpen, setFileSearchOpen] = useState(false);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "o") {
        const { rootPath, projectId } = useFileTreeStore.getState();
        if (rootPath && projectId) {
          e.preventDefault();
          setFileSearchOpen(true);
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <div className="flex h-full w-full flex-col overflow-hidden bg-[var(--color-base)]">
      <div className="flex flex-1 overflow-hidden">
        <Group orientation="horizontal" className="flex h-full w-full">
          <Panel
            defaultSize={"20%"}
            minSize={"15%"}
            maxSize={"40%"}
            className="bg-[var(--color-mantle)]"
          >
            <LeftSidebar />
          </Panel>
          <Separator className="w-px bg-[var(--color-surface0)] transition-colors hover:bg-[var(--color-blue)]" />
          <Panel
            defaultSize={"60%"}
            minSize={"30%"}
            className="bg-[var(--color-base)]"
          >
            <MainArea />
          </Panel>
          {rightSidebarVisible && (
            <>
              <Separator className="w-px bg-[var(--color-surface0)] transition-colors hover:bg-[var(--color-blue)]" />
              <Panel
                defaultSize={"20%"}
                minSize={"15%"}
                maxSize={"40%"}
                className="bg-[var(--color-mantle)]"
              >
                <RightSidebar />
              </Panel>
            </>
          )}
        </Group>
      </div>
      <StatusBar />
      <DevNotificationContainer />
      <FileSearchDialog isOpen={fileSearchOpen} onClose={() => setFileSearchOpen(false)} />
    </div>
  );
}
