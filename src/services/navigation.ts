import { monaco } from "../utils/monacoSetup";
import { useEditorStore } from "../stores/editorStore";
import { useFileTreeStore } from "../stores/fileTreeStore";

let installed = false;

export function installEditorOpener() {
  if (installed) return;
  installed = true;

  monaco.editor.registerEditorOpener({
    openCodeEditor: async (source, resource, selectionOrPosition) => {
      const path = resource.path;
      const sourcePath = source.getModel()?.uri.path;

      if (path === sourcePath) {
        return false;
      }

      const { openFiles, openFile, setActive, setPendingReveal } =
        useEditorStore.getState();
      const projectId =
        openFiles.find((f) => f.path === sourcePath)?.projectId ??
        openFiles.find((f) => f.path === path)?.projectId ??
        useFileTreeStore.getState().projectId;
      if (!projectId) return false;

      let line = 1;
      let column = 1;
      if (selectionOrPosition) {
        if ("startLineNumber" in selectionOrPosition) {
          line = selectionOrPosition.startLineNumber;
          column = selectionOrPosition.startColumn;
        } else {
          line = selectionOrPosition.lineNumber;
          column = selectionOrPosition.column;
        }
      }
      setPendingReveal({ path, line, column });

      if (openFiles.some((f) => f.path === path)) {
        setActive(path);
      } else {
        await openFile(projectId, path);
      }
      return true;
    },
  });
}
