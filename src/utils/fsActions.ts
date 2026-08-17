import { invoke } from "../services/ipc";
import { useEditorStore } from "../stores/editorStore";
import { notifyWatchedFiles, FileChangeType } from "../services/lsp/coordinator";

export async function newFile(projectId: string, fullPath: string) {
  await invoke("fs_write_file", { projectId, path: fullPath, content: "" });
  notifyWatchedFiles(projectId, [
    { path: fullPath, type: FileChangeType.Created },
  ]);
}

export async function newFolder(projectId: string, fullPath: string) {
  await invoke("fs_mkdir", { projectId, path: fullPath });
  notifyWatchedFiles(projectId, [
    { path: fullPath, type: FileChangeType.Created },
  ]);
}

export async function renameEntry(projectId: string, oldPath: string, newPath: string) {
  await invoke("fs_mv", { projectId, from: oldPath, to: newPath });
  useEditorStore.getState().remapPath(oldPath, newPath);
  notifyWatchedFiles(projectId, [
    { path: oldPath, type: FileChangeType.Deleted },
    { path: newPath, type: FileChangeType.Created },
  ]);
}

export async function deleteEntry(projectId: string, path: string, recursive: boolean) {
  await invoke("fs_rm", { projectId, path, recursive });
  useEditorStore.getState().closeUnderPath(path);
  notifyWatchedFiles(projectId, [
    { path, type: FileChangeType.Deleted },
  ]);
}
