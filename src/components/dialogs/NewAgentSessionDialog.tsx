import { useState, useEffect, useMemo } from "react";
import { invoke } from "../../services/ipc";
import { Loader2, AlertCircle, Bot, GitBranch } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import { useTerminalStore } from "../../stores/terminalStore";
import { useAgentStore } from "../../stores/agentStore";
import { listAgentModels, buildAgentCommand } from "../../services/agents";
import { AgentId, AgentModel, AgentStatus } from "../../types";
import SearchableSelect from "../ui/SearchableSelect";
import Dialog from "../ui/Dialog";

interface NewAgentSessionDialogProps {
  projectId: string;
  initialWorktreeId?: string;
  onClose: () => void;
}

// Providers offered in the dialog. Extend this list (and the model
// catalogue in agents.rs) when more coding CLIs should be launchable.
const SUPPORTED_AGENTS: AgentId[] = ["claude", "omp", "opencode"];

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

function randomWorktreeName(): string {
  const suffix = crypto.randomUUID().slice(0, 8);
  return `wt-${suffix}`;
}

function worktreeLabel(w: { id: string; branch: string; path: string; isMain: boolean }): string {
  if (w.isMain) return "local";
  return w.path.split(/[\\/]/).filter(Boolean).pop() || w.id;
}

export default function NewAgentSessionDialog({
  projectId,
  initialWorktreeId,
  onClose,
}: NewAgentSessionDialogProps) {
  const { projects, addWorktree, setActiveWorktree, fetchWorktrees, updateProject } = useProjectStore();
  const { addSession } = useTerminalStore();
  const agentsLoading = useAgentStore((s) => s.isLoading);
  const [agents, setAgents] = useState<AgentStatus[]>([]);
  const [selectedAgentId, setSelectedAgentId] = useState<AgentId | "">("");
  const [models, setModels] = useState<AgentModel[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [selectedModel, setSelectedModel] = useState("");
  const [prompt, setPrompt] = useState("");
  const [selectedBranch, setSelectedBranch] = useState("");
  const [availableBranches, setAvailableBranches] = useState<{ name: string; isRemote: boolean }[]>([]);
  const [branchesLoading, setBranchesLoading] = useState(false);
  const [branchesError, setBranchesError] = useState<string | null>(null);
  const [worktreeName, setWorktreeName] = useState("");
  const [worktreeNameDirty, setWorktreeNameDirty] = useState(false);
  const [createNew, setCreateNew] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const project = projects.find((p) => p.id === projectId);
  const worktrees = project?.worktrees ?? [];
  const existingNames = worktrees.map((w) => w.id);

  const existingWorktree = worktrees.find((w) => w.branch === selectedBranch);
  const willCreate = Boolean(selectedBranch) && (createNew || !existingWorktree);
  // Auto-fill only for branches with no worktree yet. For the local/main
  // branch or branches that already have a worktree the field stays empty
  // and the name is generated on submit when left blank.
const autoName = willCreate && !existingWorktree;
  const effectiveWorktreeName = autoName
    ? worktreeName || generateWorktreeName(selectedBranch, existingNames)
    : worktreeName;

  useEffect(() => {
    // Installed agents are loaded once at app startup (main.tsx) and cached
    // in the agent store; pick the stored preference or the first available.
    const all = useAgentStore.getState().agents;
    const available = all.filter(
      (a) => a.installed && SUPPORTED_AGENTS.includes(a.id),
    );
    setAgents(available);
    const preferred = useProjectStore
      .getState()
      .projects.find((p) => p.id === projectId)?.preferredAgent;
    const preferredAgent = available.find((a) => a.id === preferred);
    if (preferredAgent) {
      setSelectedAgentId(preferredAgent.id);
    } else if (available.length > 0) {
      setSelectedAgentId(available[0].id);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  useEffect(() => {
    if (!selectedAgentId) {
      setModels([]);
      setSelectedModel("");
      return;
    }
    let cancelled = false;
    setModelsLoading(true);
    setSelectedModel("");
    listAgentModels(selectedAgentId)
      .then((m) => {
        if (cancelled) return;
        setModels(m);
        const preferred = useProjectStore
          .getState()
          .projects.find((p) => p.id === projectId)?.preferredModel;
        if (preferred && m.some((x) => x.id === preferred)) {
          setSelectedModel(preferred);
        }
      })
      .catch(() => {
        if (!cancelled) setModels([]);
      })
      .finally(() => {
        if (!cancelled) setModelsLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedAgentId, projectId]);

  useEffect(() => {
    if (!project) return;
    let branch = "";
    const byId = worktrees.find((w) => w.id === initialWorktreeId);
    if (byId) {
      branch = byId.branch;
    } else {
      const active = worktrees.find((w) => w.id === project.activeWorktreeId);
      branch = (active ?? worktrees.find((w) => w.isMain) ?? worktrees[0])?.branch ?? "";
    }
    if (branch) setSelectedBranch(branch);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  useEffect(() => {
    fetchWorktrees(projectId).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  useEffect(() => {
    setBranchesLoading(true);
    setBranchesError(null);
    invoke<{ name: string; isRemote: boolean }[]>(
      "git_branches_available_for_worktrees_async",
      { projectId },
    )
      .then((b) => setAvailableBranches(b))
      .catch((e) => setBranchesError(String(e)))
      .finally(() => setBranchesLoading(false));
  }, [projectId]);

  useEffect(() => {
    setCreateNew(false);
    setWorktreeNameDirty(false);
    setWorktreeName("");
  }, [selectedBranch]);

  useEffect(() => {
    if (autoName && !worktreeNameDirty) {
      setWorktreeName(generateWorktreeName(selectedBranch, existingNames));
    }
  }, [selectedBranch, autoName, worktreeNameDirty, existingNames]);

  const agentOptions = agents.map((a) => ({
    value: a.id,
    label: a.label,
  }));

  const modelOptions = useMemo(
    () => [
      { value: "", label: "Default model" },
      ...models.map((m) => ({ value: m.id, label: m.label })),
    ],
    [models],
  );

  const worktreeBranches = new Set(worktrees.map((w) => w.branch));
  const branchOptions = useMemo(() => {
    const owned = worktrees.map((w) => ({
      value: w.branch,
      label: w.isMain ? `${w.branch} (main)` : `${w.branch} (${worktreeLabel(w)})`,
    }));
    const free = availableBranches
      .filter((b) => !worktreeBranches.has(b.name))
      .map((b) => ({
        value: b.name,
        label: b.isRemote ? `${b.name} (remote)` : b.name,
      }));
    const seen = new Set<string>();
    return [...owned, ...free].filter((o) => {
      if (seen.has(o.value)) return false;
      seen.add(o.value);
      return true;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [worktrees, availableBranches]);

  const canSubmit = selectedAgentId && prompt.trim() && selectedBranch;

  const handleSubmit = async () => {
    if (!canSubmit) return;
    setLoading(true);
    setError(null);
    try {
      const argv = await buildAgentCommand(
        selectedAgentId as AgentId,
        selectedModel || null,
        prompt.trim(),
      );

      let worktreeId: string;
      if (willCreate) {
        // Empty name falls back to a random name; when the branch already
        // has a worktree this becomes the derived branch.
        const finalName = effectiveWorktreeName.trim() || randomWorktreeName();
        if (existingWorktree) {
          // Branch is checked out elsewhere; git forbids a second checkout of
          // the same branch, so derive a new branch from it.
          await addWorktree(
            projectId,
            finalName,
            finalName,
            false,
            selectedBranch,
          );
        } else {
          await addWorktree(projectId, selectedBranch, finalName, false);
        }
        const refreshed = useProjectStore.getState().projects.find((p) => p.id === projectId);
        const created = refreshed?.worktrees.find((w) => w.id === finalName);
        if (!created) {
          throw new Error(
            `Worktree "${finalName}" was not found after creation`,
          );
        }
        worktreeId = created.id;
      } else {
        if (!existingWorktree) {
          throw new Error("Selected worktree was not found");
        }
        worktreeId = existingWorktree.id;
      }

      await setActiveWorktree(projectId, worktreeId);
      const wt = useProjectStore
        .getState()
        .projects.find((p) => p.id === projectId)
        ?.worktrees.find((w) => w.id === worktreeId);
      if (!wt) {
        throw new Error("Worktree was not found");
      }
      const projectType = project?.type === "ssh" ? "ssh" : "local";
      await addSession(wt.path, projectType, projectId, worktreeId, argv);
      updateProject(projectId, {
        preferredAgent: selectedAgentId as AgentId,
        preferredModel: selectedModel || null,
      }).catch(() => {});
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog
      title="Start Agent Session"
      icon={<Bot size={16} className="text-[var(--color-mauve)]" />}
      width="520px"
      scrollable
      onClose={onClose}
      footer={
        <>
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
                Starting...
              </span>
            ) : (
              "Start Session"
            )}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">
            Provider
          </label>
          <SearchableSelect
            value={selectedAgentId}
            options={agentOptions}
            onChange={(v) => setSelectedAgentId(v as AgentId)}
            placeholder="Select provider..."
            searchPlaceholder="Search provider..."
            emptyMessage="No coding agents installed (claude, omp, opencode)"
            loading={agentsLoading}
            loadingMessage="Searching coding agents..."
          />
        </div>

        <div>
          <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">
            Model <span className="text-[var(--color-overlay0)]">(optional)</span>
          </label>
          <SearchableSelect
            value={selectedModel}
            options={modelOptions}
            onChange={setSelectedModel}
            placeholder="Default model..."
            searchPlaceholder="Search model..."
            emptyMessage="No models available"
            loading={modelsLoading}
            loadingMessage="Loading models..."
          />
        </div>

        <div>
          <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">
            Initial Prompt
          </label>
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            rows={4}
            placeholder="Describe the task for the agent..."
            className="w-full resize-y rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
          />
        </div>

        <div>
          <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">
            Branch
          </label>
          <SearchableSelect
            value={selectedBranch}
            options={branchOptions}
            onChange={setSelectedBranch}
            placeholder="Select a branch..."
            searchPlaceholder="Search branch..."
            emptyMessage="No branches found"
            loading={branchesLoading}
            loadingMessage="Loading branches..."
          />
          {branchesError && (
            <div className="mt-1.5 flex items-center gap-1.5 text-xs text-[var(--color-peach)]">
              <AlertCircle size={12} />
              {branchesError}
            </div>
          )}
          {existingWorktree && (
            <div className="mt-3 space-y-3 rounded-md border border-[var(--color-surface0)] bg-[var(--color-surface0)]/30 p-3">
              <div className="flex gap-2">
                <button
                  onClick={() => setCreateNew(false)}
                  className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                    !createNew
                      ? "bg-[var(--color-blue)]/20 text-[var(--color-blue)]"
                      : "bg-[var(--color-surface0)] text-[var(--color-overlay1)] hover:bg-[var(--color-surface1)]"
                  }`}
                >
                  Use existing worktree
                </button>
                <button
                  onClick={() => setCreateNew(true)}
                  className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                    createNew
                      ? "bg-[var(--color-blue)]/20 text-[var(--color-blue)]"
                      : "bg-[var(--color-surface0)] text-[var(--color-overlay1)] hover:bg-[var(--color-surface1)]"
                  }`}
                >
                  New worktree
                </button>
              </div>
              {!createNew ? (
                <div className="flex items-center gap-1.5 text-xs text-[var(--color-subtext1)]">
                  <GitBranch size={12} className="text-[var(--color-green)]" />
                  Uses existing worktree{" "}
                  <span className="font-mono">{worktreeLabel(existingWorktree)}</span>
                </div>
              ) : (
                <div>
                  <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">
                    Worktree Name{" "}
                    <span className="text-[var(--color-overlay0)]">
                      (optional, auto-generated)
                    </span>
                  </label>
                  <input
                    type="text"
                    value={worktreeName}
                    onChange={(e) => {
                      setWorktreeNameDirty(true);
                      setWorktreeName(
                        e.target.value.replace(/\s/g, "-").replace(/[^a-zA-Z0-9_-]/g, ""),
                      );
                    }}
                    pattern="[a-zA-Z0-9_-]*"
                    placeholder="random name"
                    className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
                  />
                </div>
              )}
            </div>
          )}
          {!existingWorktree && willCreate && (
            <div className="mt-3 space-y-3 rounded-md border border-[var(--color-surface0)] bg-[var(--color-surface0)]/30 p-3">
              <p className="flex items-center gap-1.5 text-xs text-[var(--color-subtext1)]">
                <GitBranch size={12} className="text-[var(--color-mauve)]" />
                No worktree for this branch yet — a new one will be created
              </p>
              <div>
                <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">
                  Worktree Name{" "}
                  <span className="text-[var(--color-overlay0)]">
                    (optional, auto-generated)
                  </span>
                </label>
                <input
                  type="text"
                  value={worktreeName}
                  onChange={(e) => {
                    setWorktreeNameDirty(true);
                    setWorktreeName(
                      e.target.value.replace(/\s/g, "-").replace(/[^a-zA-Z0-9_-]/g, ""),
                    );
                  }}
                  pattern="[a-zA-Z0-9_-]*"
                  placeholder="random name"
                  className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
                />
              </div>
            </div>
          )}
        </div>

        {error && (
          <div className="flex items-center gap-2 text-sm text-[var(--color-peach)]">
            <AlertCircle size={14} />
            {error}
          </div>
        )}
      </div>
    </Dialog>
  );
}
