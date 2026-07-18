import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EyeOff, Eye, RefreshCw, Loader2, FilePlus, FolderPlus } from "lucide-react";
import { useFileTreeStore } from "../../stores/fileTreeStore";
import FileTreeNode from "./FileTreeNode";
import FileTreeContextMenu from "./FileTreeContextMenu";
import NewItemDialog from "../dialogs/NewItemDialog";
import RenameDialog from "../dialogs/RenameDialog";
import DeleteDialog from "../dialogs/DeleteDialog";

export default function FileTree() {
  const { rootPath, projectId, nodeState, showIgnored, setShowIgnored, refreshDir } = useFileTreeStore();
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; entryPath: string; isDir: boolean; name: string } | null>(null);
  const [newItem, setNewItem] = useState<{ type: "file" | "folder"; parent: string } | null>(null);
  const [renameItem, setRenameItem] = useState<{ entryPath: string; isDir: boolean; name: string } | null>(null);
  const [deleteItem, setDeleteItem] = useState<{ entryPath: string; isDir: boolean; name: string } | null>(null);
  const stableNewFile = useCallback(() => setNewItem({ type: "file", parent: rootPath ?? "" }), [rootPath]);
  const stableNewFolder = useCallback(() => setNewItem({ type: "folder", parent: rootPath ?? "" }), [rootPath]);

  if (!rootPath || !projectId) {
    return (
      <div className="flex flex-1 items-center justify-center p-4">
        <span className="text-sm text-[var(--color-overlay0)]">No project selected</span>
      </div>
    );
  }

  const rootState = nodeState[rootPath];
  const isRootLoading = rootState?.loading ?? false;
  const isRootError = rootState?.error;
  const rootChildren = rootState?.children ?? [];

  const handleCreateItem = async (type: "file" | "folder", parent: string, name: string) => {
    const cmd = type === "folder" ? "fs_mkdir" : "fs_write_file";
    const args: Record<string, unknown> = type === "folder"
      ? { projectId, path: `${parent}/${name}` }
      : { projectId, path: `${parent}/${name}`, content: "" };
    try {
      // @ts-ignore
      await invoke(cmd, args);
      refreshDir(parent);
    } catch (e) {
      alert(`${e}`);
    }
    setNewItem(null);
  };

  const handleRename = async (oldPath: string, newName: string) => {
    const parent = oldPath.substring(0, oldPath.lastIndexOf("/"));
    try {
      await invoke("fs_mv", { projectId, from: oldPath, to: `${parent}/${newName}` });
      setRenameItem(null);
      refreshDir(parent);
    } catch (e) {
      alert(`Failed to rename: ${e}`);
      setRenameItem(null);
    }
  };

  const handleDelete = async (entryPath: string, isDir: boolean) => {
    const parent = entryPath.substring(0, entryPath.lastIndexOf("/"));
    try {
      await invoke("fs_rm", { projectId, path: entryPath, recursive: isDir });
      setDeleteItem(null);
      refreshDir(parent);
    } catch (e) {
      alert(`Failed to delete: ${e}`);
      setDeleteItem(null);
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center gap-1 border-b border-[var(--color-surface0)] px-3 py-2">
        <button
          onClick={() => setShowIgnored(!showIgnored)}
          className="rounded p-1 text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)] hover:text-[var(--color-text)]"
          title={showIgnored ? "Hide ignored files" : "Show ignored files"}
        >
          {showIgnored ? <Eye size={14} /> : <EyeOff size={14} />}
        </button>
        <button
          onClick={() => refreshDir(rootPath)}
          className="rounded p-1 text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)] hover:text-[var(--color-text)]"
          title="Refresh"
        >
          <RefreshCw size={14} />
        </button>
        <div className="flex-1" />
        <button
          onClick={stableNewFile}
          className="rounded p-1 text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)] hover:text-[var(--color-text)]"
          title="New file"
        >
          <FilePlus size={14} />
        </button>
        <button
          onClick={stableNewFolder}
          className="rounded p-1 text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)] hover:text-[var(--color-text)]"
          title="New folder"
        >
          <FolderPlus size={14} />
        </button>
      </div>

      {/* Tree */}
      <div className="flex-1 overflow-y-auto overflow-x-hidden pb-2 select-none">
        {isRootLoading && (
          <div className="sticky top-0 z-10 flex items-center gap-2 bg-[var(--color-mantle)] px-3 py-1 text-xs text-[var(--color-overlay1)]">
            <Loader2 size={12} className="animate-spin" />
            Refreshing…
          </div>
        )}
        {isRootError && (
          <div className="px-3 py-4 text-sm text-[var(--color-red)]">{isRootError}</div>
        )}
        {rootChildren.map((child) => (
          <FileTreeNode
            key={child.name}
            entry={child}
            dirPath={rootPath}
            depth={0}
            onContextMenu={(e, entryPath, isDir, name) => {
              e.preventDefault();
              e.stopPropagation();
              setContextMenu({ x: e.clientX, y: e.clientY, entryPath, isDir, name });
            }}
          />
        ))}
        {rootChildren.length === 0 && !isRootLoading && !isRootError && (
          <div className="px-3 py-4 text-sm text-[var(--color-overlay0)]">Empty</div>
        )}
      </div>

      {/* Context menu */}
      {contextMenu && (
        <FileTreeContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          entryPath={contextMenu.entryPath}
          name={contextMenu.name}
          isDir={contextMenu.isDir}
          onClose={() => setContextMenu(null)}
          onNewFile={(parent) => setNewItem({ type: "file", parent })}
          onNewFolder={(parent) => setNewItem({ type: "folder", parent })}
          onRename={(path, _isDir, name) => { setContextMenu(null); setRenameItem({ entryPath: path, isDir: _isDir, name }); }}
          onDelete={(path, _isDir, name) => { setContextMenu(null); setDeleteItem({ entryPath: path, isDir: _isDir, name }); }}
          onCopyPath={async (path) => {
            await navigator.clipboard.writeText(path);
            setContextMenu(null);
          }}
          onCopyRelativePath={async () => {
            const rel = contextMenu.entryPath === rootPath ? '.' : contextMenu.entryPath.startsWith(rootPath + '/') ? contextMenu.entryPath.slice(rootPath.length + 1) : contextMenu.entryPath;
            await navigator.clipboard.writeText(rel);
            setContextMenu(null);
          }}
        />
      )}

      {/* Dialogs */}
      {newItem && (
        <NewItemDialog
          itemType={newItem.type}
          parentPath={newItem.parent}
          onConfirm={(name) => handleCreateItem(newItem.type, newItem.parent, name)}
          onCancel={() => setNewItem(null)}
        />
      )}
      {renameItem && (
        <RenameDialog
          currentName={renameItem.name}
          isDir={renameItem.isDir}
          onConfirm={(name) => handleRename(renameItem.entryPath, name)}
          onCancel={() => setRenameItem(null)}
        />
      )}
      {deleteItem && (
        <DeleteDialog
          name={deleteItem.name}
          isDir={deleteItem.isDir}
          onConfirm={() => handleDelete(deleteItem.entryPath, deleteItem.isDir)}
          onCancel={() => setDeleteItem(null)}
        />
      )}
    </div>
  );
}
