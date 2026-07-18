import { invoke } from "@tauri-apps/api/core";

export async function newFile(projectId: string, fullPath: string) {
  await invoke("fs_write_file", { projectId, path: fullPath, content: "" });
}

export async function newFolder(projectId: string, fullPath: string) {
  await invoke("fs_mkdir", { projectId, path: fullPath });
}

export async function renameEntry(projectId: string, oldPath: string, newPath: string) {
  await invoke("fs_mv", { projectId, from: oldPath, to: newPath });
}

export async function deleteEntry(projectId: string, path: string, recursive: boolean) {
  await invoke("fs_rm", { projectId, path, recursive });
}
