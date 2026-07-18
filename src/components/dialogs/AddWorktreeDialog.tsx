import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X, Loader2, AlertCircle } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";

interface AddWorktreeDialogProps {
  projectId: string;
  onClose: () => void;
}

function generateWorktreeName(branch: string, existingNames: string[]): string {
  const base = branch.replace(/\//g, "-").replace(/[^a-zA-Z0-9-_]/g, "");
  let name = base;
  let i = 1;
  while (existingNames.includes(name)) {
    name = `${base}-${i}`;
    i++;
  }
  return name;
}

export default function AddWorktreeDialog({ projectId, onClose }: AddWorktreeDialogProps) {
  const { projects, addWorktree } = useProjectStore();
  const [branches, setBranches] = useState<{ name: string; isRemote: boolean }[]>([]);
  const [loading, setLoading] = useState(false);
  const [mode, setMode] = useState<"existing" | "new">("existing");
  const [selectedBranch, setSelectedBranch] = useState("");
  const [newBranchName, setNewBranchName] = useState("");
  const [worktreeName, setWorktreeName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [branchesError, setBranchesError] = useState<string | null>(null);

  const project = projects.find((p) => p.id === projectId);
  const existingNames = project?.worktrees.map((w) => w.id) || [];

  const branch = mode === "existing" ? selectedBranch : newBranchName;
  const isNew = mode === "new";
  const effectiveName = worktreeName || generateWorktreeName(branch, existingNames);

  useEffect(() => {
    setBranchesError(null);
    invoke<{ name: string; isRemote: boolean }[]>("git_branches_list_async", { projectId })
      .then((b) => {
        setBranches(b);
        setBranchesError(null);
      })
      .catch((e) => setBranchesError(String(e)));
  }, [projectId]);

  useEffect(() => {
    if (branch && !worktreeName) {
      const generated = generateWorktreeName(branch, existingNames);
      setWorktreeName(generated);
    }
  }, [branch]);

  const canSubmit = branch && effectiveName;

  const handleSubmit = async () => {
    if (!canSubmit) return;
    setLoading(true);
    setError(null);
    try {
      await addWorktree(projectId, branch, effectiveName, isNew);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="flex w-[480px] max-h-[80vh] flex-col rounded-lg border border-[var(--color-surface0)] bg-[var(--color-mantle)] shadow-2xl">
        <div className="flex items-center justify-between border-b border-[var(--color-surface0)] px-4 py-3">
          <h2 className="text-sm font-semibold text-[var(--color-text)]">Add Worktree</h2>
          <button onClick={onClose} className="text-[var(--color-overlay1)] hover:text-[var(--color-text)]">
            <X size={18} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          <div>
            <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">Branch</label>
            <div className="flex gap-2 mb-2">
              <button
                onClick={() => setMode("existing")}
                className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                  mode === "existing"
                    ? "bg-[var(--color-blue)]/20 text-[var(--color-blue)]"
                    : "bg-[var(--color-surface0)] text-[var(--color-overlay1)] hover:bg-[var(--color-surface1)]"
                }`}
              >
                Existing Branch
              </button>
              <button
                onClick={() => setMode("new")}
                className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                  mode === "new"
                    ? "bg-[var(--color-blue)]/20 text-[var(--color-blue)]"
                    : "bg-[var(--color-surface0)] text-[var(--color-overlay1)] hover:bg-[var(--color-surface1)]"
                }`}
              >
                New Branch
              </button>
            </div>
            {mode === "existing" ? (
              <>
                <select
                  value={selectedBranch}
                  onChange={(e) => setSelectedBranch(e.target.value)}
                  className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] focus:border-[var(--color-blue)] focus:outline-none"
                >
                  <option value="">Select a branch...</option>
                  {branches.map((b) => (
                    <option key={b.name} value={b.name}>
                      {b.isRemote ? `↗ ${b.name}` : b.name}
                    </option>
                  ))}
                </select>
                {branchesError && (
                  <div className="mt-1.5 flex items-center gap-1.5 text-xs text-[var(--color-peach)]">
                    <AlertCircle size={12} />
                    {branchesError}
                  </div>
                )}
              </>
            ) : (
              <input
                type="text"
                value={newBranchName}
                onChange={(e) => setNewBranchName(e.target.value)}
                placeholder="feature/my-branch"
                className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
              />
            )}
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">
              Worktree Name <span className="text-[var(--color-overlay0)]">(optional, auto-generated)</span>
            </label>
            <input
              type="text"
              value={worktreeName}
              onChange={(e) => setWorktreeName(e.target.value)}
              placeholder="auto-generated from branch"
              className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
            />
          </div>

          {error && (
            <div className="flex items-center gap-2 text-sm text-[var(--color-peach)]">
              <AlertCircle size={14} />
              {error}
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-[var(--color-surface0)] px-4 py-3">
          <button
            onClick={onClose}
            className="rounded-md px-4 py-2 text-sm text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]"
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={!canSubmit || loading}
            className="rounded-md bg-[var(--color-blue)] px-4 py-2 text-sm font-medium text-[var(--color-crust)] transition-colors hover:bg-[var(--color-blue)]/80 disabled:opacity-50"
          >
            {loading ? (
              <span className="flex items-center gap-2">
                <Loader2 size={14} className="animate-spin" />
                Creating...
              </span>
            ) : (
              "Create Worktree"
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
