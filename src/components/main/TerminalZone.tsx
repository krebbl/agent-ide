import {
  Terminal,
  Plus,
  ChevronDown,
  ChevronUp,
  ChevronRight,
  Loader2,
} from "lucide-react";
import { useEffect, useMemo, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTerminalStore, collectLeaves, findLeaf } from "../../stores/terminalStore";
import { useProjectStore } from "../../stores/projectStore";
import { useUiStore } from "../../stores/uiStore";
import SplitPaneContainer from "./SplitPaneContainer";
import TabStrip from "../ui/TabStrip";
import LoadingOverlay from "../ui/LoadingOverlay";

function tabHasBusySession(
  tab: import("../../types").TerminalTab,
  sessions: import("../../stores/terminalStore").TerminalSession[],
): boolean {
  const leaves = collectLeaves(tab.rootPane);
  return leaves.some((leaf) => {
    const session = sessions.find((s) => s.id === leaf.sessionId);
    return session?.isBusy === true;
  });
}

function tabHasUnseenActivity(
  tab: import("../../types").TerminalTab,
  sessions: import("../../stores/terminalStore").TerminalSession[],
): boolean {
  const leaves = collectLeaves(tab.rootPane);
  return leaves.some((leaf) => {
    const session = sessions.find((s) => s.id === leaf.sessionId);
    return session?.hasUnseenActivity === true;
  });
}

function getFocusedSessionTitle(
  tab: import("../../types").TerminalTab,
  sessions: import("../../stores/terminalStore").TerminalSession[],
): string {
  const leaf = findLeaf(tab.rootPane, tab.focusedPaneId);
  if (!leaf) return "Terminal";
  const session = sessions.find((s) => s.id === leaf.sessionId);
  return session?.title ?? "Terminal";
}

interface TerminalZoneProps {
  isCollapsed: boolean;
  onToggleCollapse: () => void;
}

export default function TerminalZone({
  isCollapsed,
  onToggleCollapse,
}: TerminalZoneProps) {
  const sessions = useTerminalStore((s) => s.sessions);
  const tabs = useTerminalStore((s) => s.tabs);
  const activeTabId = useTerminalStore((s) => s.activeTabId);
  const activeSessionId = useTerminalStore((s) => s.activeSessionId);
  const addSession = useTerminalStore((s) => s.addSession);
  const removeSession = useTerminalStore((s) => s.removeSession);
  const splitPane = useTerminalStore((s) => s.splitPane);
  const navigatePane = useTerminalStore((s) => s.navigatePane);

  const activeProjectId = useProjectStore((s) => s.activeProjectId);
  const selectedWorktreeId = useProjectStore((s) => s.selectedWorktreeId);
  const worktreeLoading = useUiStore((s) => s.worktreeLoading);

  const visibleTabs = useMemo(
    () =>
      tabs.filter(
        (t) =>
          t.projectId === activeProjectId && t.worktreeId === selectedWorktreeId,
      ),
    [tabs, activeProjectId, selectedWorktreeId],
  );

  const activeTab = useMemo(
    () => visibleTabs.find((t) => t.id === activeTabId) ?? null,
    [visibleTabs, activeTabId],
  );

  const effectiveActiveId =
    activeTab?.id ??
    visibleTabs[visibleTabs.length - 1]?.id ??
    null;

  useEffect(() => {
    if (!activeTab && visibleTabs.length > 0 && effectiveActiveId) {
      let tab: (typeof visibleTabs)[number] | undefined;
      if (activeProjectId && selectedWorktreeId) {
        const savedTabId = useTerminalStore
          .getState()
          .getWorktreeTabId(activeProjectId, selectedWorktreeId);
        if (savedTabId) {
          tab = visibleTabs.find((t) => t.id === savedTabId);
        }
      }
      if (!tab) {
        tab = visibleTabs[visibleTabs.length - 1];
      }
      const focused = findLeaf(tab.rootPane, tab.focusedPaneId);
      if (focused) {
        useTerminalStore.setState({
          activeTabId: tab.id,
          activeSessionId: focused.sessionId,
        });
      }
    }
  }, [activeTab, visibleTabs, effectiveActiveId, activeProjectId, selectedWorktreeId]);

  useEffect(() => {
    if (isCollapsed) {
      invoke("pty_set_active", { ptyId: null }).catch(() => {});
      return;
    }
    if (activeSessionId) {
      const session = sessions.find((s) => s.id === activeSessionId);
      if (session) {
        invoke("pty_set_active", { ptyId: session.ptyId }).catch(() => {});
        return;
      }
    }
    invoke("pty_set_active", { ptyId: null }).catch(() => {});
  }, [activeSessionId, sessions, isCollapsed]);

  const canAddTerminal = activeProjectId && selectedWorktreeId;

  const handleNewTerminal = useCallback(() => {
    if (!canAddTerminal) return;
    addSession(undefined, undefined, activeProjectId, selectedWorktreeId).catch(
      () => {},
    );
  }, [canAddTerminal, activeProjectId, selectedWorktreeId, addSession]);

  const handleCloseTab = useCallback(
    (tabId: string) => {
      const tab = tabs.find((t) => t.id === tabId);
      if (!tab) return;
      const leaves = collectLeaves(tab.rootPane);
      leaves.forEach((leaf) => {
        removeSession(leaf.sessionId).catch(() => {});
      });
    },
    [tabs, removeSession],
  );

  const handleCloseActivePane = useCallback(() => {
    if (!activeTab || !activeTab.focusedPaneId) return;
    const leaf = findLeaf(activeTab.rootPane, activeTab.focusedPaneId);
    if (leaf) {
      removeSession(leaf.sessionId).catch(() => {});
    }
  }, [activeTab, removeSession]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const isMod = e.metaKey || e.ctrlKey;
      if (!isMod) return;
      if (useUiStore.getState().focusedZone !== "terminal") return;

      if (e.key.toLowerCase() === "t") {
        e.preventDefault();
        handleNewTerminal();
        return;
      }

      if (e.key.toLowerCase() === "w") {
        e.preventDefault();
        handleCloseActivePane();
        return;
      }

      if (e.key.toLowerCase() === "d" && !e.shiftKey && activeSessionId) {
        e.preventDefault();
        splitPane(activeSessionId, "horizontal").catch(() => {});
        return;
      }

      if (e.key.toLowerCase() === "d" && e.shiftKey && activeSessionId) {
        e.preventDefault();
        splitPane(activeSessionId, "vertical").catch(() => {});
        return;
      }

      if (e.altKey) {
        switch (e.key) {
          case "ArrowLeft":
            e.preventDefault();
            navigatePane("left");
            break;
          case "ArrowRight":
            e.preventDefault();
            navigatePane("right");
            break;
          case "ArrowUp":
            e.preventDefault();
            navigatePane("up");
            break;
          case "ArrowDown":
            e.preventDefault();
            navigatePane("down");
            break;
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [handleNewTerminal, handleCloseActivePane, activeSessionId, splitPane, navigatePane]);

  return (
    <div
      className="flex h-full flex-col"
      onPointerDown={() => useUiStore.getState().setFocusedZone("terminal")}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-[var(--color-surface0)] px-2">
        <Terminal size={14} className="text-[var(--color-green)]" />
        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
          Terminal
        </span>

        <TabStrip
          tabs={visibleTabs.map((tab) => {
            const isActive = tab.id === effectiveActiveId;
            const isBusy = tabHasBusySession(tab, sessions);
            const hasUnseen = tabHasUnseenActivity(tab, sessions);
            const title = getFocusedSessionTitle(tab, sessions);
            const leafCount = collectLeaves(tab.rootPane).length;
            return {
              id: tab.id,
              title,
              tooltip: title,
              icon: isBusy ? (
                <Loader2
                  size={12}
                  className="animate-spin text-[var(--color-blue)]"
                  aria-label="Busy"
                />
              ) : hasUnseen && !isActive ? (
                <ChevronRight
                  size={12}
                  className="animate-blink text-[var(--color-blue)]"
                  aria-label="Ready — unseen"
                />
              ) : (
                <ChevronRight
                  size={12}
                  className="text-[var(--color-green)]"
                  aria-label="Ready"
                />
              ),
              badge:
                leafCount > 1 ? (
                  <span className="text-[10px] text-[var(--color-overlay0)]">
                    {leafCount}
                  </span>
                ) : undefined,
            };
          })}
          activeId={effectiveActiveId}
          onSelect={(tabId) => {
            const tab = visibleTabs.find((t) => t.id === tabId);
            if (!tab) return;
            if (tabHasUnseenActivity(tab, sessions) && tabId !== effectiveActiveId) {
              const leaves = collectLeaves(tab.rootPane);
              leaves.forEach((leaf) => {
                useTerminalStore.getState().markSessionSeen(leaf.sessionId);
              });
            }
            useTerminalStore.getState().focusSession(
              findLeaf(tab.rootPane, tab.focusedPaneId)?.sessionId ?? "",
            );
          }}
          onClose={handleCloseTab}
        />

        <button
          onClick={handleNewTerminal}
          disabled={!canAddTerminal}
          className="shrink-0 text-[var(--color-overlay1)] transition-colors hover:text-[var(--color-blue)] disabled:opacity-40 disabled:cursor-not-allowed"
          title="New terminal (Cmd+T)"
        >
          <Plus size={16} />
        </button>

        <button
          onClick={onToggleCollapse}
          className="shrink-0 text-[var(--color-overlay1)] transition-colors hover:text-[var(--color-text)]"
          title={isCollapsed ? "Expand terminal" : "Collapse terminal"}
        >
          {isCollapsed ? <ChevronDown size={16} /> : <ChevronUp size={16} />}
        </button>
      </div>

      <div className="relative flex-1 overflow-hidden bg-[var(--color-base)]">
        {worktreeLoading && <LoadingOverlay label="Connecting to remote…" />}
        {visibleTabs.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-4">
            <span className="text-sm text-[var(--color-overlay0)]">
              {canAddTerminal
                ? "No terminal sessions for this worktree"
                : "Select a worktree to open a terminal"}
            </span>
            {canAddTerminal && (
              <button
                onClick={handleNewTerminal}
                className="flex items-center gap-1.5 rounded-md bg-[var(--color-surface0)] px-3 py-1.5 text-xs text-[var(--color-subtext0)] transition-colors hover:bg-[var(--color-surface1)] hover:text-[var(--color-text)]"
              >
                <Plus size={14} />
                New Terminal
              </button>
            )}
          </div>
        )}
        {tabs.map((tab) => {
          const tabMatchesWorktree =
            tab.projectId === activeProjectId &&
            tab.worktreeId === selectedWorktreeId;
          const isActiveTab = tabMatchesWorktree && tab.id === effectiveActiveId;
          return (
            <div
              key={tab.id}
              className={
                isActiveTab
                  ? "h-full w-full"
                  : "absolute inset-0 hidden"
              }
            >
              <SplitPaneContainer
                pane={tab.rootPane}
                focusedPaneId={tab.focusedPaneId}
                isWorktreeHidden={!tabMatchesWorktree}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}