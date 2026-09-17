import { useState } from "react";
import { Settings } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import { useJiraStore } from "../../stores/jiraStore";
import { useUiStore } from "../../stores/uiStore";
import Dialog from "../ui/Dialog";

function SectionLabel({ children }: { children: string }) {
  return (
    <div className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
      {children}
    </div>
  );
}

export default function ProjectSettingsDialog() {
  const projectId = useUiStore((s) => s.projectSettingsProjectId);
  const closeProjectSettings = useUiStore((s) => s.closeProjectSettings);
  const project = useProjectStore((s) =>
    s.projects.find((p) => p.id === projectId),
  );
  const config = useJiraStore((s) => (projectId ? s.configs[projectId] : undefined));
  const saveConfig = useJiraStore((s) => s.saveConfig);
  const updateProject = useProjectStore((s) => s.updateProject);

  const [siteUrl, setSiteUrl] = useState(config?.siteUrl ?? project?.jiraConfig?.siteUrl ?? "");
  const [email, setEmail] = useState(config?.email ?? project?.jiraConfig?.email ?? "");
  const [apiToken, setApiToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!projectId || !project) return null;

  const hasToken = config?.hasToken ?? false;
  const canSave =
    !!siteUrl.trim() && !!email.trim() && (hasToken || !!apiToken.trim());

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const update = await saveConfig(projectId, siteUrl, email, apiToken || undefined);
      await updateProject(projectId, { jiraConfig: update.project.jiraConfig });
      closeProjectSettings();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSaving(false);
    }
  };

  return (
    <Dialog
      title={`Project Settings — ${project.name}`}
      icon={<Settings size={16} className="text-[var(--color-subtext0)]" />}
      onClose={closeProjectSettings}
      onCmdEnter={() => canSave && !saving && void save()}
      footer={
        <>
          <button
            onClick={closeProjectSettings}
            className="rounded border border-[var(--color-surface0)] px-3 py-1.5 text-xs text-[var(--color-subtext0)] hover:text-[var(--color-text)]"
          >
            Cancel
          </button>
          <button
            onClick={() => void save()}
            disabled={saving || !canSave}
            className="rounded bg-[var(--color-blue)] px-3 py-1.5 text-xs font-medium text-[var(--color-crust)] transition-opacity hover:opacity-90 disabled:opacity-40"
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <SectionLabel>Jira</SectionLabel>
        <label className="flex flex-col gap-1 text-xs text-[var(--color-subtext1)]">
          Site URL
          <input
            value={siteUrl}
            onChange={(e) => setSiteUrl(e.target.value)}
            placeholder="https://yourcompany.atlassian.net"
            className="rounded border border-[var(--color-surface0)] bg-[var(--color-base)] px-2 py-1.5 text-xs text-[var(--color-text)] outline-none focus:border-[var(--color-blue)]"
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-[var(--color-subtext1)]">
          Email
          <input
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="you@yourcompany.com"
            className="rounded border border-[var(--color-surface0)] bg-[var(--color-base)] px-2 py-1.5 text-xs text-[var(--color-text)] outline-none focus:border-[var(--color-blue)]"
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-[var(--color-subtext1)]">
          API token
          <input
            type="password"
            value={apiToken}
            onChange={(e) => setApiToken(e.target.value)}
            placeholder={hasToken ? "Stored — leave empty to keep" : "Create at id.atlassian.com/manage-profile/security/api-tokens"}
            className="rounded border border-[var(--color-surface0)] bg-[var(--color-base)] px-2 py-1.5 text-xs text-[var(--color-text)] outline-none focus:border-[var(--color-blue)]"
          />
        </label>
        {error && <div className="text-xs text-[var(--color-red)]">{error}</div>}
      </div>
    </Dialog>
  );
}
