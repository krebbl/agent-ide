import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DiffEditor } from "@monaco-editor/react";
import {
  AlertCircle,
  ChevronRight,
  FilePlus2,
  FileQuestion,
  GitCompare,
  Loader2,
  MessageSquarePlus,
  Pencil,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { monaco } from "../../utils/monacoSetup";
import { languageFromPath } from "../../stores/editorStore";
import { useDiffStore, diffSelectionState } from "../../stores/diffStore";
import { useProjectStore } from "../../stores/projectStore";
import { useUiStore } from "../../stores/uiStore";
import type { DiffCommentSide, DiffFileEntry, DiffFileStatus } from "../../types";

const STATUS_STYLES: Record<DiffFileStatus, { color: string; label: string }> = {
  added: { color: "text-[var(--color-green)]", label: "A" },
  modified: { color: "text-[var(--color-blue)]", label: "M" },
  deleted: { color: "text-[var(--color-red)]", label: "D" },
  renamed: { color: "text-[var(--color-mauve)]", label: "R" },
  conflicted: { color: "text-[var(--color-yellow)]", label: "!" },
};

interface Draft {
  side: DiffCommentSide;
  line: number;
}

interface FileTreeNode {
  name: string;
  /** Full repo-relative path (files) or directory path (folders). */
  path: string;
  children: FileTreeNode[];
  file?: DiffFileEntry;
}

function buildFileTree(files: DiffFileEntry[]): FileTreeNode[] {
  const root: FileTreeNode[] = [];
  const dirs = new Map<string, FileTreeNode>();
  const ensureDir = (parts: string[], depth: number): FileTreeNode[] => {
    let level = root;
    let key = "";
    for (let i = 0; i < depth; i++) {
      key = key ? `${key}/${parts[i]}` : parts[i];
      let node = dirs.get(key);
      if (!node) {
        node = { name: parts[i], path: key, children: [] };
        dirs.set(key, node);
        level.push(node);
      }
      level = node.children;
    }
    return level;
  };
  for (const file of files) {
    const parts = file.path.split("/");
    const level = ensureDir(parts, parts.length - 1);
    level.push({
      name: parts[parts.length - 1],
      path: file.path,
      children: [],
      file,
    });
  }
  const sort = (nodes: FileTreeNode[]) => {
    nodes.sort((a, b) => {
      const aDir = a.children.length > 0 ? 0 : 1;
      const bDir = b.children.length > 0 ? 0 : 1;
      if (aDir !== bDir) return aDir - bDir;
      return a.name.localeCompare(b.name);
    });
    nodes.forEach((n) => {
      if (n.children.length > 0) sort(n.children);
    });
  };
  sort(root);
  return root;
}

function countFiles(node: FileTreeNode): number {
  if (node.children.length === 0) return 1;
  return node.children.reduce((n, c) => n + countFiles(c), 0);
}

function TreeRow({
  node,
  depth,
  collapsedDirs,
  toggleDir,
  selectedPath,
  filesWithComments,
  selectFile,
}: {
  node: FileTreeNode;
  depth: number;
  collapsedDirs: Set<string>;
  toggleDir: (path: string) => void;
  selectedPath: string | null;
  filesWithComments: Set<string>;
  selectFile: (path: string) => void;
}) {
  const indent = { paddingLeft: `${10 + depth * 12}px` };
  if (node.children.length > 0) {
    const collapsed = collapsedDirs.has(node.path);
    return (
      <div>
        <button
          onClick={() => toggleDir(node.path)}
          className="flex w-full items-center gap-1 py-1.5 pr-3 text-left text-xs text-[var(--color-subtext0)] transition-colors hover:bg-[var(--color-surface0)]/60"
          style={indent}
        >
          <ChevronRight
            size={12}
            className={`shrink-0 text-[var(--color-overlay1)] transition-transform ${collapsed ? "" : "rotate-90"}`}
          />
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
          <span className="shrink-0 text-[10px] text-[var(--color-overlay0)]">
            {countFiles(node)}
          </span>
        </button>
        {!collapsed &&
          node.children.map((child) => (
            <TreeRow
              key={child.path}
              node={child}
              depth={depth + 1}
              collapsedDirs={collapsedDirs}
              toggleDir={toggleDir}
              selectedPath={selectedPath}
              filesWithComments={filesWithComments}
              selectFile={selectFile}
            />
          ))}
      </div>
    );
  }
  const file = node.file;
  if (!file) return null;
  const style = STATUS_STYLES[file.status] ?? STATUS_STYLES.modified;
  const isActive = file.path === selectedPath;
  return (
    <button
      onClick={() => selectFile(file.path)}
      className={`flex w-full items-center gap-2 pr-3 py-1.5 text-left text-xs transition-colors ${
        isActive
          ? "bg-[var(--color-surface0)] text-[var(--color-text)]"
          : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface0)]/60"
      }`}
      style={indent}
      title={file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}
    >
      <span className={`font-mono text-[10px] ${style.color}`}>{style.label}</span>
      <span className="min-w-0 flex-1 truncate font-mono">{node.name}</span>
      {(file.insertions > 0 || file.deletions > 0) && (
        <span className="shrink-0 font-mono text-[10px] text-[var(--color-overlay0)]">
          +{file.insertions} −{file.deletions}
        </span>
      )}
      {filesWithComments.has(file.path) && (
        <MessageSquarePlus size={11} className="shrink-0 text-[var(--color-blue)]" />
      )}
    </button>
  );
}

export default function DiffPanel() {
  const {
    projectId,
    worktreeId,
    branch,
    worktreePath,
    mode,
    baseBranch,
    files,
    selectedPath,
    diffCache,
    comments,
    filesLoading,
    diffLoading,
    error,
    openFor,
    close,
    refresh,
    setMode,
    selectFile,
    addComment,
    updateComment,
    deleteComment,
  } = useDiffStore();
  const activeProjectId = useProjectStore((s) => s.activeProjectId);
  const selectedWorktreeId = useProjectStore((s) => s.selectedWorktreeId);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [draftBody, setDraftBody] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editBody, setEditBody] = useState("");
  const [saving, setSaving] = useState(false);
  const diffEditorRef = useRef<monaco.editor.IStandaloneDiffEditor | null>(null);
  const originalDecorationsRef = useRef<monaco.editor.IEditorDecorationsCollection | null>(null);
  const modifiedDecorationsRef = useRef<monaco.editor.IEditorDecorationsCollection | null>(null);

  // Follow worktree switches while the panel is open — but only when the
  // active selection actually CHANGES. On mount we adopt the current
  // selection without retargeting, so an explicit "Show Changes" target
  // (e.g. a non-active worktree via the context menu) is never snapped
  // back to the active worktree's diff.
  useEffect(() => {
    if (!useUiStore.getState().diffPanelOpen) return;
    if (!activeProjectId || !selectedWorktreeId) return;
    const key = `${activeProjectId}:${selectedWorktreeId}`;
    // Retarget only when we previously observed a DIFFERENT selection while
    // the panel was open. On mount (lastSeen null) adopt the current
    // selection, so an explicit "Show Changes" target is never snapped back.
    if (diffSelectionState.lastSeen !== null && diffSelectionState.lastSeen !== key) {
      openFor(activeProjectId, selectedWorktreeId);
    }
    diffSelectionState.lastSeen = key;
  }, [activeProjectId, selectedWorktreeId, projectId, worktreeId, openFor]);

  const content = selectedPath ? diffCache[selectedPath] : undefined;
  const fileComments = comments.filter((c) => c.file === selectedPath);
  const filesWithComments = useMemo(
    () => new Set(comments.map((c) => c.file)),
    [comments],
  );
  const fileTree = useMemo(() => buildFileTree(files), [files]);
  const [collapsedDirs, setCollapsedDirs] = useState<Set<string>>(() => new Set());
  const toggleDir = (path: string) => {
    setCollapsedDirs((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  const applyDecorations = useCallback(() => {
    const editor = diffEditorRef.current;
    if (!editor) return;
    const lineFor = (side: DiffCommentSide) =>
      fileComments
        .filter((c) => c.side === side && c.resolvedLine !== null)
        .map((c) => ({
          range: new monaco.Range(c.resolvedLine!, 1, c.resolvedLine!, 1),
          options: { isWholeLine: true, linesDecorationsClassName: "diff-comment-marker" },
        }));
    if (!originalDecorationsRef.current) {
      originalDecorationsRef.current = editor.getOriginalEditor().createDecorationsCollection();
    }
    if (!modifiedDecorationsRef.current) {
      modifiedDecorationsRef.current = editor.getModifiedEditor().createDecorationsCollection();
    }
    originalDecorationsRef.current.set(lineFor("base"));
    modifiedDecorationsRef.current.set(lineFor("modified"));
  }, [fileComments]);

  useEffect(() => {
    applyDecorations();
  }, [selectedPath, comments, applyDecorations]);

  const handleMount = (editor: monaco.editor.IStandaloneDiffEditor) => {
    diffEditorRef.current = editor;
    const clickLine = (side: DiffCommentSide) => (e: monaco.editor.IEditorMouseEvent) => {
      const line = e.target.position?.lineNumber;
      if (line) setDraft({ side, line });
    };
    editor.getOriginalEditor().onMouseDown(clickLine("base"));
    editor.getModifiedEditor().onMouseDown(clickLine("modified"));
    applyDecorations();
  };

  const submitDraft = async () => {
    if (!draft || !selectedPath || !draftBody.trim() || saving) return;
    setSaving(true);
    try {
      await addComment(selectedPath, draft.side, draft.line, draftBody);
      setDraft(null);
      setDraftBody("");
    } catch {
      // keep draft on failure
    } finally {
      setSaving(false);
    }
  };

  const submitEdit = async () => {
    if (!editingId || !editBody.trim() || saving) return;
    setSaving(true);
    try {
      await updateComment(editingId, editBody);
      setEditingId(null);
      setEditBody("");
    } catch {
      // keep edit on failure
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-full w-full flex-col bg-[var(--color-base)]">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-[var(--color-surface0)] px-3">
        <GitCompare size={13} className="text-[var(--color-blue)]" />
        <span className="text-xs font-medium text-[var(--color-text)]">Changes</span>
        {branch && (
          <span className="rounded bg-[var(--color-surface0)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--color-subtext1)]">
            {branch}
          </span>
        )}
        {files.length > 0 && (
          <span className="text-[10px] text-[var(--color-overlay1)]">
            {files.length} file{files.length === 1 ? "" : "s"}
          </span>
        )}
        {baseBranch && baseBranch !== branch && (
          <div className="ml-2 flex items-center gap-0.5 rounded bg-[var(--color-surface0)] p-0.5">
            <button
              onClick={() => setMode("worktree").catch(() => {})}
              className={`rounded px-1.5 py-0.5 text-[10px] transition-colors ${
                mode === "worktree"
                  ? "bg-[var(--color-blue)]/20 text-[var(--color-blue)]"
                  : "text-[var(--color-overlay1)] hover:text-[var(--color-text)]"
              }`}
            >
              Working tree
            </button>
            <button
              onClick={() => setMode("branch").catch(() => {})}
              className={`rounded px-1.5 py-0.5 text-[10px] transition-colors ${
                mode === "branch"
                  ? "bg-[var(--color-blue)]/20 text-[var(--color-blue)]"
                  : "text-[var(--color-overlay1)] hover:text-[var(--color-text)]"
              }`}
            >
              vs {baseBranch}
            </button>
          </div>
        )}
        <div className="ml-auto flex items-center gap-1">
          <button
            onClick={() => refresh().catch(() => {})}
            className="rounded p-1 text-[var(--color-overlay1)] transition-colors hover:bg-[var(--color-surface0)] hover:text-[var(--color-text)]"
            title="Refresh changes"
          >
            <RefreshCw size={13} className={filesLoading ? "animate-spin" : ""} />
          </button>
          <button
            onClick={close}
            className="rounded p-1 text-[var(--color-overlay1)] transition-colors hover:bg-[var(--color-surface0)] hover:text-[var(--color-text)]"
            title="Close changes panel"
          >
            <X size={13} />
          </button>
        </div>
      </div>

      {error && (
        <div className="flex items-center gap-1.5 border-b border-[var(--color-surface0)] bg-[var(--color-red)]/10 px-3 py-1.5 text-xs text-[var(--color-red)]">
          <AlertCircle size={12} />
          {error}
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <div className="w-64 shrink-0 overflow-y-auto border-r border-[var(--color-surface0)]">
          {!filesLoading && files.length === 0 && !error && (
            <p className="px-3 py-4 text-xs text-[var(--color-overlay0)]">
              {mode === "branch"
                ? `No changes vs ${baseBranch ?? "base branch"}`
                : "No uncommitted changes"}
            </p>
          )}
          {fileTree.map((node) => (
            <TreeRow
              key={node.path}
              node={node}
              depth={0}
              collapsedDirs={collapsedDirs}
              toggleDir={toggleDir}
              selectedPath={selectedPath}
              filesWithComments={filesWithComments}
              selectFile={selectFile}
            />
          ))}
        </div>

        <div className="flex min-w-0 flex-1 flex-col">
          {!selectedPath ? (
            <div className="flex flex-1 items-center justify-center text-xs text-[var(--color-overlay0)]">
              Select a file to view its diff
            </div>
          ) : content?.binary ? (
            <div className="flex flex-1 items-center justify-center text-xs text-[var(--color-overlay0)]">
              Binary file — no diff shown
            </div>
          ) : diffLoading || !content ? (
            <div className="flex flex-1 items-center justify-center text-xs text-[var(--color-overlay0)]">
              <Loader2 size={14} className="mr-2 animate-spin" />
              Loading diff…
            </div>
          ) : (
            <DiffEditor
              key={selectedPath}
              original={content.base ?? ""}
              modified={content.modified ?? ""}
              language={languageFromPath(selectedPath)}
              theme="catppuccin-mocha"
              onMount={handleMount}
              options={{
                readOnly: true,
                renderSideBySide: true,
                minimap: { enabled: false },
                fontSize: 12,
                scrollBeyondLastLine: false,
                originalEditable: false,
              }}
            />
          )}

          {draft && selectedPath && (
            <div className="shrink-0 border-t border-[var(--color-surface0)] p-2">
              <div className="mb-1 flex items-center gap-2 text-[10px] text-[var(--color-subtext1)]">
                <span>
                  Comment on {draft.side === "base" ? "original" : "modified"} line {draft.line}
                </span>
                <button
                  onClick={() => {
                    setDraft(null);
                    setDraftBody("");
                  }}
                  className="ml-auto text-[var(--color-overlay1)] hover:text-[var(--color-text)]"
                >
                  <X size={11} />
                </button>
              </div>
              <div className="flex gap-2">
                <textarea
                  autoFocus
                  value={draftBody}
                  onChange={(e) => setDraftBody(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) submitDraft();
                  }}
                  rows={2}
                  placeholder="Add a comment (⌘Enter to save)"
                  className="min-w-0 flex-1 resize-y rounded border border-[var(--color-surface0)] bg-[var(--color-base)] px-2 py-1.5 text-xs text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
                />
                <button
                  onClick={submitDraft}
                  disabled={!draftBody.trim() || saving}
                  className="self-end rounded bg-[var(--color-blue)] px-3 py-1.5 text-xs font-medium text-[var(--color-crust)] transition-colors hover:bg-[var(--color-blue)]/80 disabled:opacity-50"
                >
                  {saving ? <Loader2 size={12} className="animate-spin" /> : "Add"}
                </button>
              </div>
            </div>
          )}

          {selectedPath && fileComments.length > 0 && (
            <div className="max-h-40 shrink-0 overflow-y-auto border-t border-[var(--color-surface0)]">
              {fileComments.map((c) => (
                <div
                  key={c.id}
                  className="border-b border-[var(--color-surface0)] px-3 py-2 last:border-b-0"
                >
                  {editingId === c.id ? (
                    <div className="flex gap-2">
                      <textarea
                        autoFocus
                        value={editBody}
                        onChange={(e) => setEditBody(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) submitEdit();
                        }}
                        rows={2}
                        className="min-w-0 flex-1 resize-y rounded border border-[var(--color-surface0)] bg-[var(--color-base)] px-2 py-1.5 text-xs text-[var(--color-text)] focus:border-[var(--color-blue)] focus:outline-none"
                      />
                      <div className="flex flex-col gap-1">
                        <button
                          onClick={submitEdit}
                          disabled={!editBody.trim() || saving}
                          className="rounded bg-[var(--color-blue)] px-2 py-1 text-[10px] font-medium text-[var(--color-crust)] hover:bg-[var(--color-blue)]/80 disabled:opacity-50"
                        >
                          Save
                        </button>
                        <button
                          onClick={() => setEditingId(null)}
                          className="rounded bg-[var(--color-surface0)] px-2 py-1 text-[10px] text-[var(--color-overlay1)] hover:bg-[var(--color-surface1)]"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-start gap-2">
                      <div className="min-w-0 flex-1">
                        <div className="mb-0.5 flex items-center gap-1.5 text-[10px] text-[var(--color-overlay0)]">
                          <span className={c.side === "base" ? "text-[var(--color-mauve)]" : "text-[var(--color-green)]"}>
                            {c.side === "base" ? "original" : "modified"}
                          </span>
                          <span>line {c.resolvedLine ?? c.line}</span>
                          {c.orphan && (
                            <span className="flex items-center gap-0.5 text-[var(--color-yellow)]">
                              <FileQuestion size={10} />
                              moved
                            </span>
                          )}
                          {new Date(c.updatedAt).toLocaleDateString()}
                        </div>
                        <p className="whitespace-pre-wrap text-xs text-[var(--color-text)]">{c.body}</p>
                      </div>
                      <div className="flex shrink-0 gap-0.5">
                        <button
                          onClick={() => {
                            setEditingId(c.id);
                            setEditBody(c.body);
                          }}
                          className="rounded p-1 text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)] hover:text-[var(--color-text)]"
                          title="Edit comment"
                        >
                          <Pencil size={11} />
                        </button>
                        <button
                          onClick={() => deleteComment(c.id).catch(() => {})}
                          className="rounded p-1 text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)] hover:text-[var(--color-red)]"
                          title="Delete comment"
                        >
                          <Trash2 size={11} />
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {worktreePath === null && (
        <div className="flex flex-1 items-center justify-center">
          <FilePlus2 size={16} className="text-[var(--color-overlay0)]" />
        </div>
      )}
    </div>
  );
}
