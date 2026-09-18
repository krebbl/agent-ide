import { create } from "zustand";
import { invoke } from "../services/ipc";
import type {
  DiffComment,
  DiffCommentSide,
  DiffFileContent,
  DiffFileEntry,
} from "../types";
import { useProjectStore } from "./projectStore";
import { useUiStore } from "./uiStore";

interface DiffState {
  projectId: string | null;
  worktreeId: string | null;
  branch: string | null;
  worktreePath: string | null;
  /** "worktree": uncommitted changes vs HEAD. "branch": committed changes
   *  from the merge base with baseBranch up to HEAD (PR review view). */
  mode: "worktree" | "branch";
  baseBranch: string | null;
  /** True once the user picked a mode in the panel; disables auto-fallback. */
  modeExplicit: boolean;
  ahead: number;
  files: DiffFileEntry[];
  selectedPath: string | null;
  diffCache: Record<string, DiffFileContent>;
  comments: DiffComment[];
  filesLoading: boolean;
  diffLoading: boolean;
  error: string | null;
  openFor: (projectId: string, worktreeId?: string) => void;
  close: () => void;
  refresh: () => Promise<void>;
  setMode: (mode: "worktree" | "branch") => Promise<void>;
  selectFile: (path: string) => Promise<void>;
  addComment: (
    file: string,
    side: DiffCommentSide,
    line: number,
    body: string,
  ) => Promise<void>;
  updateComment: (id: string, body: string) => Promise<void>;
  deleteComment: (id: string) => Promise<void>;
}

let refreshSeq = 0;

/** Last active project+worktree selection the DiffPanel observed while open.
 *  Lives here so close() can reset it: a selection made while the panel is
 *  closed must be adopted on the next open, never retargeted away from an
 *  explicit "Show Changes" target. */
export const diffSelectionState = { lastSeen: null as string | null };

function resolveWorktree(projectId: string, worktreeId?: string) {
  const { projects } = useProjectStore.getState();
  const project = projects.find((p) => p.id === projectId);
  if (!project) return null;
  const worktree =
    project.worktrees.find((w) => w.id === worktreeId) ??
    project.worktrees.find((w) => w.id === project.activeWorktreeId) ??
    project.worktrees.find((w) => w.isMain) ??
    project.worktrees[0];
  if (!worktree) return null;
  return { project, worktree };
}

async function loadComments(
  projectId: string,
  branch: string,
  worktreePath: string,
): Promise<DiffComment[]> {
  try {
    return await invoke<DiffComment[]>("diff_comments_list", {
      projectId,
      branch,
      worktreePath,
    });
  } catch {
    return [];
  }
}

export const useDiffStore = create<DiffState>((set, get) => ({
  projectId: null,
  worktreeId: null,
  branch: null,
  worktreePath: null,
  mode: "worktree",
  baseBranch: null,
  modeExplicit: false,
  ahead: 0,
  files: [],
  selectedPath: null,
  diffCache: {},
  comments: [],
  filesLoading: false,
  diffLoading: false,
  error: null,

  openFor: (projectId, worktreeId) => {
    const resolved = resolveWorktree(projectId, worktreeId);
    if (!resolved) return;
    const { project, worktree } = resolved;
    const changed =
      get().projectId !== project.id ||
      get().worktreeId !== worktree.id;
    set({
      projectId: project.id,
      worktreeId: worktree.id,
      branch: worktree.branch,
      worktreePath: worktree.path,
      mode: "worktree",
      modeExplicit: false,
      baseBranch: project.worktrees.find((w) => w.isMain)?.branch ?? null,
      ahead: worktree.ahead ?? 0,
      ...(changed ? { files: [], selectedPath: null, diffCache: {}, comments: [] } : {}),
      error: null,
    });
    useUiStore.getState().setDiffPanelOpen(true);
    get().refresh().catch(() => {});
  },

  close: () => {
    useUiStore.getState().setDiffPanelOpen(false);
    diffSelectionState.lastSeen = null;
  },

  refresh: async () => {
    const { projectId, branch, worktreePath, mode, baseBranch, modeExplicit } = get();
    if (!projectId || !branch || !worktreePath) return;
    const seq = ++refreshSeq;
    set({ filesLoading: true, error: null });
    try {
      const files = await invoke<DiffFileEntry[]>("git_diff_summary", {
        projectId,
        worktreePath,
        baseBranch: mode === "branch" ? baseBranch : null,
      });
      // A newer openFor/refresh superseded this request; drop the result.
      if (seq !== refreshSeq) return;
      // A clean worktree on a feature branch has its interesting changes
      // committed (e.g. an open PR) — show those instead of an empty list.
      if (
        mode === "worktree" &&
        !modeExplicit &&
        files.length === 0 &&
        baseBranch &&
        baseBranch !== branch
      ) {
        set({ mode: "branch" });
        get().refresh().catch(() => {});
        return;
      }
      const comments = await loadComments(projectId, branch, worktreePath);
      if (seq !== refreshSeq) return;
      set((s) => ({
        files,
        comments,
        filesLoading: false,
        selectedPath:
          s.selectedPath && files.some((f) => f.path === s.selectedPath)
            ? s.selectedPath
            : (files[0]?.path ?? null),
      }));
      const { selectedPath, diffCache } = get();
      if (selectedPath && !diffCache[selectedPath]) {
        get().selectFile(selectedPath).catch(() => {});
      }
    } catch (e) {
      if (seq !== refreshSeq) return;
      set({ files: [], filesLoading: false, error: String(e) });
    }
  },

  setMode: async (mode) => {
    if (get().mode === mode) return;
    set({ mode, modeExplicit: true, files: [], selectedPath: null, diffCache: {} });
    await get().refresh();
  },

  selectFile: async (path) => {
    set({ selectedPath: path });
    if (get().diffCache[path]) return;
    const { projectId, worktreePath, mode, baseBranch } = get();
    if (!projectId || !worktreePath) return;
    set({ diffLoading: true });
    try {
      const content = await invoke<DiffFileContent>("git_file_diff", {
        projectId,
        worktreePath,
        path,
        baseBranch: mode === "branch" ? baseBranch : null,
      });
      set((s) => ({
        diffCache: { ...s.diffCache, [path]: content },
        diffLoading: false,
      }));
    } catch (e) {
      set({ diffLoading: false, error: String(e) });
    }
  },

  addComment: async (file, side, line, body) => {
    const { projectId, branch, worktreePath } = get();
    if (!projectId || !branch || !worktreePath) return;
    const comments = await invoke<DiffComment[]>("diff_comment_add", {
      projectId,
      branch,
      file,
      side,
      line,
      body,
    });
    set({ comments });
  },

  updateComment: async (id, body) => {
    const { projectId, branch, worktreePath } = get();
    if (!projectId || !branch || !worktreePath) return;
    const comments = await invoke<DiffComment[]>("diff_comment_update", {
      commentId: id,
      body,
    });
    set({ comments });
  },

  deleteComment: async (id) => {
    const { projectId, branch, worktreePath } = get();
    if (!projectId || !branch || !worktreePath) return;
    const comments = await invoke<DiffComment[]>("diff_comment_delete", {
      commentId: id,
    });
    set({ comments });
  },
}));
