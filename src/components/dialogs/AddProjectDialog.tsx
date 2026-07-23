import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Server, Key, Lock, CheckCircle, AlertCircle, Loader2 } from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import { Project, SSHConnection, LocalConnection } from "../../types";
import RemoteDirBrowser from "./RemoteDirBrowser";
import Dialog from "../ui/Dialog";

interface AddProjectDialogProps {
  onClose: () => void;
}

export default function AddProjectDialog({ onClose }: AddProjectDialogProps) {
  const [tab, setTab] = useState<"local" | "ssh">("local");
  const { addProject } = useProjectStore();

  const [localPath, setLocalPath] = useState("");
  const [localName, setLocalName] = useState("");
  const [localIsGit, setLocalIsGit] = useState<boolean | null>(null);
  const [localChecking, setLocalChecking] = useState(false);

  const [sshHost, setSshHost] = useState("marcuskrejpowicz.com");
  const [sshPort, setSshPort] = useState(22);
  const [sshUsername, setSshUsername] = useState("dev");
  const [sshAuthMethod, setSshAuthMethod] = useState<"key" | "password" | "agent">("key");
  const [sshKeyPath, setSshKeyPath] = useState("");
  const [sshPassword, setSshPassword] = useState("");
  const [sshTestStatus, setSshTestStatus] = useState<"idle" | "testing" | "success" | "error">("idle");
  const [sshTestMessage, setSshTestMessage] = useState("");
  const [sshRemotePath, setSshRemotePath] = useState("/");
  const [sshName, setSshName] = useState("");
  const [sshShowBrowser, setSshShowBrowser] = useState(false);
  const [sshAgentInfo, setSshAgentInfo] = useState<{
    authSock: string | null;
    socketExists: boolean;
    onePasswordSocket: string | null;
    onePasswordSocketExists: boolean;
    agentKeyCount: number | null;
    agentKeyComments: string[];
    pubKeyCount: number;
    pubKeyComments: string[];
    error: string | null;
  } | null>(null);
  const [sshAgentChecking, setSshAgentChecking] = useState(false);

  const handleLocalBrowse = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === "string") {
      setLocalPath(selected);
      const parts = selected.split("/").filter(Boolean);
      setLocalName(parts[parts.length - 1] || selected);
      setLocalIsGit(null);
      setLocalChecking(true);
      try {
        const isGit = await invoke<boolean>("check_is_git_repo", { path: selected });
        setLocalIsGit(isGit);
      } catch {
        setLocalIsGit(false);
      } finally {
        setLocalChecking(false);
      }
    }
  };

  const handleLocalInit = async () => {
    if (!localPath) return;
    try {
      await invoke("git_init", { path: localPath });
      setLocalIsGit(true);
    } catch (e) {
      alert(String(e));
    }
  };

  useEffect(() => {
    if (sshAuthMethod !== "agent") {
      setSshAgentInfo(null);
      return;
    }
    setSshAgentChecking(true);
    invoke<{
      auth_sock: string | null;
      socket_exists: boolean;
      one_password_socket: string | null;
      one_password_socket_exists: boolean;
      agent_key_count: number | null;
      agent_key_comments: string[];
      pub_key_count: number;
      pub_key_comments: string[];
      error: string | null;
    }>("ssh_agent_info")
      .then((info) => {
        setSshAgentInfo({
          authSock: info.auth_sock,
          socketExists: info.socket_exists,
          onePasswordSocket: info.one_password_socket,
          onePasswordSocketExists: info.one_password_socket_exists,
          agentKeyCount: info.agent_key_count,
          agentKeyComments: info.agent_key_comments,
          pubKeyCount: info.pub_key_count,
          pubKeyComments: info.pub_key_comments,
          error: info.error,
        });
      })
      .catch((e) => {
        setSshAgentInfo({
          authSock: null,
          socketExists: false,
          onePasswordSocket: null,
          onePasswordSocketExists: false,
          agentKeyCount: null,
          agentKeyComments: [],
          pubKeyCount: 0,
          pubKeyComments: [],
          error: String(e),
        });
      })
      .finally(() => setSshAgentChecking(false));
  }, [sshAuthMethod]);

  const handleSshTest = async () => {
    if (!sshHost || !sshUsername) return;
    setSshTestStatus("testing");
    setSshTestMessage("");
    try {
      await invoke("ssh_test_connection", {
        host: sshHost,
        port: sshPort,
        username: sshUsername,
        authMethod: sshAuthMethod,
        keyPath: sshAuthMethod === "key" ? sshKeyPath || null : null,
        password: sshAuthMethod === "password" ? sshPassword || null : null,
      });
      setSshTestStatus("success");
      setSshTestMessage("Connection successful");
    } catch (e) {
      setSshTestStatus("error");
      setSshTestMessage(String(e));
    }
  };

  const handleSshBrowse = () => {
    setSshShowBrowser(true);
  };

  const handleSshSelectPath = (path: string) => {
    setSshRemotePath(path);
    const derived = path.split("/").filter(Boolean).pop() || "";
    if (!sshName) setSshName(derived);
    setSshShowBrowser(false);
  };

  const handleAddProject = async () => {
    if (tab === "local") {
      if (!localPath || !localName) return;
      const project: Project = {
        id: crypto.randomUUID(),
        name: localName,
        type: "local",
        connection: { type: "local", path: localPath } as LocalConnection,
        worktrees: [],
        activeWorktreeId: null,
      };
      await addProject(project);
      onClose();
    } else {
      if (!sshHost || !sshUsername || !sshRemotePath) return;
      const connection: SSHConnection = {
        type: "ssh",
        host: sshHost,
        port: sshPort,
        username: sshUsername,
        authMethod: sshAuthMethod,
        keyPath: sshAuthMethod === "key" ? sshKeyPath : undefined,
        path: sshRemotePath,
      };
      const projectId = crypto.randomUUID();
      const project: Project = {
        id: projectId,
        name: sshName || sshRemotePath.split("/").filter(Boolean).pop() || sshHost,
        type: "ssh",
        connection,
        worktrees: [],
        activeWorktreeId: null,
      };
      if (sshAuthMethod === "password" && sshPassword) {
        await invoke("ssh_store_password", { projectId, password: sshPassword });
      }
      await addProject(project);
      onClose();
    }
  };

  const canAddLocal = localPath && localName && localIsGit;
  const canAddSsh = sshHost && sshUsername && sshRemotePath && sshName && sshTestStatus === "success";

  return (
    <Dialog
      title="Add Project"
      width="560px"
      scrollable
      closeOnBackdrop={false}
      onClose={onClose}
      footer={
        <>
          <button onClick={onClose} className="rounded-md px-4 py-2 text-sm text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]">
            Cancel
          </button>
          <button
            onClick={handleAddProject}
            disabled={tab === "local" ? !canAddLocal : !canAddSsh}
            className="rounded-md bg-[var(--color-blue)] px-4 py-2 text-sm font-medium text-[var(--color-crust)] transition-colors hover:bg-[var(--color-blue)]/80 disabled:opacity-50"
          >
            Add Project
          </button>
        </>
      }
    >
      <div className="flex border-b border-[var(--color-surface0)] -mx-4 -mt-4 mb-4">
        <button
          onClick={() => setTab("local")}
          className={`flex items-center gap-2 px-4 py-2.5 text-sm font-medium transition-colors ${
            tab === "local"
              ? "border-b-2 border-[var(--color-blue)] text-[var(--color-blue)]"
              : "text-[var(--color-overlay1)] hover:text-[var(--color-text)]"
          }`}
        >
          <FolderOpen size={14} />
          Local Folder
        </button>
        <button
          onClick={() => setTab("ssh")}
          className={`flex items-center gap-2 px-4 py-2.5 text-sm font-medium transition-colors ${
            tab === "ssh"
              ? "border-b-2 border-[var(--color-blue)] text-[var(--color-blue)]"
              : "text-[var(--color-overlay1)] hover:text-[var(--color-text)]"
          }`}
        >
          <Server size={14} />
          SSH Remote
        </button>
      </div>

      {tab === "local" && (
        <div className="space-y-4">
          <div>
            <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">Project Path</label>
            <div className="flex gap-2">
              <input
                type="text"
                value={localPath}
                onChange={(e) => setLocalPath(e.target.value)}
                placeholder="/path/to/project"
                className="flex-1 rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
              />
              <button
                onClick={handleLocalBrowse}
                className="rounded-md bg-[var(--color-surface0)] px-3 py-2 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface1)]"
              >
                Browse
              </button>
            </div>
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">Project Name</label>
            <input
              type="text"
              value={localName}
              onChange={(e) => setLocalName(e.target.value)}
              placeholder="Project name"
              className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
            />
          </div>

          {localChecking && (
            <div className="flex items-center gap-2 text-sm text-[var(--color-overlay1)]">
              <Loader2 size={14} className="animate-spin" />
              Checking for git repository...
            </div>
          )}

          {localIsGit === false && !localChecking && (
            <div className="rounded-md border border-[var(--color-peach)]/30 bg-[var(--color-peach)]/10 p-3">
              <div className="flex items-center gap-2 text-sm text-[var(--color-peach)]">
                <AlertCircle size={14} />
                This folder is not a git repository.
              </div>
              <div className="mt-2 flex gap-2">
                <button
                  onClick={handleLocalInit}
                  className="rounded-md bg-[var(--color-peach)]/20 px-3 py-1.5 text-xs text-[var(--color-peach)] hover:bg-[var(--color-peach)]/30"
                >
                  Initialize Git
                </button>
                <button
                  onClick={() => setLocalIsGit(null)}
                  className="rounded-md bg-[var(--color-surface0)] px-3 py-1.5 text-xs text-[var(--color-overlay1)] hover:bg-[var(--color-surface1)]"
                >
                  Cancel
                </button>
              </div>
            </div>
          )}

          {localIsGit === true && (
            <div className="flex items-center gap-2 text-sm text-[var(--color-green)]">
              <CheckCircle size={14} />
              Git repository detected
            </div>
          )}
        </div>
      )}

      {tab === "ssh" && (
        <div className="space-y-3">
          <div className="grid grid-cols-3 gap-3">
            <div className="col-span-2">
              <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">Host</label>
              <input
                type="text"
                value={sshHost}
                onChange={(e) => setSshHost(e.target.value)}
                placeholder="example.com"
                className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">Port</label>
              <input
                type="number"
                value={sshPort}
                onChange={(e) => setSshPort(Number(e.target.value))}
                className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] focus:border-[var(--color-blue)] focus:outline-none"
              />
            </div>
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">Username</label>
            <input
              type="text"
              value={sshUsername}
              onChange={(e) => setSshUsername(e.target.value)}
              placeholder="user"
              className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
            />
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">Authentication</label>
            <select
              value={sshAuthMethod}
              onChange={(e) => setSshAuthMethod(e.target.value as "key" | "password" | "agent")}
              className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] focus:border-[var(--color-blue)] focus:outline-none"
            >
              <option value="key">Private Key</option>
              <option value="agent">SSH Agent</option>
              <option value="password">Password</option>
            </select>
          </div>

          {sshAuthMethod === "key" && (
            <div>
              <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">
                <span className="flex items-center gap-1"><Key size={12} /> Private Key Path</span>
              </label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={sshKeyPath}
                  onChange={(e) => setSshKeyPath(e.target.value)}
                  placeholder="~/.ssh/id_ed25519"
                  className="flex-1 rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
                />
                <button
                  onClick={async () => {
                    const selected = await open({ multiple: false });
                    if (selected && typeof selected === "string") {
                      setSshKeyPath(selected);
                    }
                  }}
                  className="rounded-md bg-[var(--color-surface0)] px-3 py-2 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface1)]"
                >
                  Browse
                </button>
              </div>
            </div>
          )}

          {sshAuthMethod === "agent" && (
            <div className="rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] p-3 space-y-2">
              {sshAgentChecking && (
                <div className="flex items-center gap-2 text-sm text-[var(--color-overlay1)]">
                  <Loader2 size={14} className="animate-spin" />
                  Checking SSH agent...
                </div>
              )}
              {sshAgentInfo && !sshAgentChecking && (
                <>
                  {sshAgentInfo.error ? (
                    <div className="flex items-center gap-2 text-sm text-[var(--color-peach)]">
                      <AlertCircle size={14} />
                      {sshAgentInfo.error}
                    </div>
                  ) : (
                    <div className="flex items-center gap-2 text-sm text-[var(--color-green)]">
                      <CheckCircle size={14} />
                      {sshAgentInfo.agentKeyCount !== null
                        ? `${sshAgentInfo.agentKeyCount} key${sshAgentInfo.agentKeyCount !== 1 ? "s" : ""} loaded`
                        : "SSH agent ready"}
                    </div>
                  )}
                </>
              )}
            </div>
          )}

          {sshAuthMethod === "password" && (
            <div>
              <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">
                <span className="flex items-center gap-1"><Lock size={12} /> Password</span>
              </label>
              <input
                type="password"
                value={sshPassword}
                onChange={(e) => setSshPassword(e.target.value)}
                placeholder="••••••••"
                className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
              />
            </div>
          )}

          <button
            onClick={handleSshTest}
            disabled={sshTestStatus === "testing" || !sshHost || !sshUsername}
            className="w-full rounded-md bg-[var(--color-blue)]/20 px-3 py-2 text-sm font-medium text-[var(--color-blue)] transition-colors hover:bg-[var(--color-blue)]/30 disabled:opacity-50"
          >
            {sshTestStatus === "testing" ? (
              <span className="flex items-center justify-center gap-2">
                <Loader2 size={14} className="animate-spin" />
                Testing...
              </span>
            ) : (
              "Test Connection"
            )}
          </button>

          {sshTestStatus === "success" && (
            <div className="space-y-3">
              <div className="flex items-center gap-2 text-sm text-[var(--color-green)]">
                <CheckCircle size={14} />
                {sshTestMessage}
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">Remote Path</label>
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    value={sshRemotePath}
                    onChange={(e) => {
                      setSshRemotePath(e.target.value);
                      if (!sshName) {
                        const derived = e.target.value.split("/").filter(Boolean).pop() || "";
                        setSshName(derived);
                      }
                    }}
                    className="flex-1 rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] focus:border-[var(--color-blue)] focus:outline-none"
                  />
                  <button
                    onClick={handleSshBrowse}
                    className="rounded-md bg-[var(--color-surface0)] px-3 py-2 text-sm text-[var(--color-text)] hover:bg-[var(--color-surface1)]"
                  >
                    Browse
                  </button>
                </div>
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium text-[var(--color-subtext1)]">Project Name</label>
                <input
                  type="text"
                  value={sshName}
                  onChange={(e) => setSshName(e.target.value)}
                  placeholder="Project name"
                  className="w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:border-[var(--color-blue)] focus:outline-none"
                />
              </div>
            </div>
          )}

          {sshTestStatus === "error" && (
            <div className="flex items-center gap-2 text-sm text-[var(--color-red)]">
              <AlertCircle size={14} />
              {sshTestMessage}
            </div>
          )}

          {sshShowBrowser && (
            <RemoteDirBrowser
              currentPath={sshRemotePath}
              onNavigate={setSshRemotePath}
              onSelect={handleSshSelectPath}
              onCancel={() => setSshShowBrowser(false)}
            />
          )}
        </div>
      )}
    </Dialog>
  );
}