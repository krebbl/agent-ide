import { useEffect, useRef, useState } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { ClipboardAddon } from "@xterm/addon-clipboard";
import { ListTree, X } from "lucide-react";
import { invoke } from "../../services/ipc";
import { openUrl } from "../../utils/openUrl";
import { fetchSessionProcesses } from "../../services/processes";
import { ProcessInfo } from "../../types";
import {
  registerTerminal,
  registerTerminalIdle,
  registerTerminalBusy,
  registerTerminalError,
  unregisterTerminal,
} from "../../services/terminalEvents";
import { useTerminalStore } from "../../stores/terminalStore";
import { notify } from "../../services/notifications";
import "@xterm/xterm/css/xterm.css";

interface TerminalViewProps {
  sessionId: string;
  ptyId: string;
  isFocused: boolean;
  isCollapsed: boolean;
}

export default function TerminalView({
  sessionId,
  ptyId,
  isFocused,
  isCollapsed,
}: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const rafRef = useRef<number | null>(null);
  const processTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isVisible = !isCollapsed;
  const isVisibleRef = useRef(isVisible);
  const isFocusedRef = useRef(isFocused);
  const isWindowFocusedRef = useRef(document.hasFocus());
  const wasBusyRef = useRef<boolean>(false);
  const processRunningRef = useRef<boolean>(false);
  const notifiedForIdleRef = useRef<boolean>(false);
  const skipFirstIdleRef = useRef<boolean>(true);

  useEffect(() => {
    isVisibleRef.current = isVisible;
  }, [isVisible]);

  useEffect(() => {
    isFocusedRef.current = isFocused;
    const terminal = xtermRef.current;
    if (!terminal) return;
    if (isFocused) {
      useTerminalStore.getState().markSessionSeen(sessionId);
      terminal.focus();
      requestAnimationFrame(() => {
        fitAndResize(false);
        terminal.refresh(0, terminal.rows - 1);
      });
    }
  }, [isFocused, sessionId]);

  useEffect(() => {
    const handleFocus = () => {
      isWindowFocusedRef.current = true;
    };
    const handleBlur = () => {
      isWindowFocusedRef.current = false;
    };
    window.addEventListener("focus", handleFocus);
    window.addEventListener("blur", handleBlur);
    return () => {
      window.removeEventListener("focus", handleFocus);
      window.removeEventListener("blur", handleBlur);
    };
  }, []);

  const shouldNotify = () =>
    !isVisibleRef.current || !isWindowFocusedRef.current;

  const setActivity = (activity: { isBusy: boolean; needsInput: boolean }) => {
    useTerminalStore.getState().setSessionActivity(sessionId, activity);
  };

  const notifyIdle = (title: string) => {
    setActivity({ isBusy: false, needsInput: true });
    if (skipFirstIdleRef.current) {
      skipFirstIdleRef.current = false;
      notifiedForIdleRef.current = true;
      wasBusyRef.current = false;
      return;
    }
    if (notifiedForIdleRef.current) return;
    if (!wasBusyRef.current) return;

    const { activeSessionId } = useTerminalStore.getState();
    if (sessionId !== activeSessionId) {
      useTerminalStore.getState().setSessionUnseenActivity(sessionId, true);
    }

    if (shouldNotify()) {
      notify({
        title: "Terminal ready",
        body: ` "${title}" has finished.`,
        sessionId,
      });
    }
    notifiedForIdleRef.current = true;
    wasBusyRef.current = false;
  };

  const resetIdleState = () => {
    notifiedForIdleRef.current = false;
  };

  const endProcess = () => {
    processRunningRef.current = false;
    wasBusyRef.current = false;
    if (processTimeoutRef.current) {
      clearTimeout(processTimeoutRef.current);
      processTimeoutRef.current = null;
    }
    useTerminalStore.getState().setProcessRunning(sessionId, false);
  };

  const handleIdle = (title: string) => {
    notifyIdle(title);
    endProcess();
  };

  const handleBusy = () => {
    startProcess();
  };

  const startProcess = () => {
    processRunningRef.current = true;
    wasBusyRef.current = true;
    resetIdleState();
    useTerminalStore.getState().setProcessRunning(sessionId, true);
    setActivity({ isBusy: true, needsInput: false });
    if (processTimeoutRef.current) {
      clearTimeout(processTimeoutRef.current);
    }
    processTimeoutRef.current = setTimeout(() => {
      const session = useTerminalStore
        .getState()
        .sessions.find((s) => s.id === sessionId);
      notifyIdle(session?.title ?? "Terminal");
      endProcess();
    }, 1500);
  };

  const extendProcess = () => {
    if (!processRunningRef.current) {
      processRunningRef.current = true;
    }
    if (processTimeoutRef.current) {
      clearTimeout(processTimeoutRef.current);
    }
    processTimeoutRef.current = setTimeout(() => {
      const session = useTerminalStore
        .getState()
        .sessions.find((s) => s.id === sessionId);
      notifyIdle(session?.title ?? "Terminal");
      endProcess();
    }, 1500);
  };

  const fitAndResize = (resize = false) => {
    const terminal = xtermRef.current;
    const fitAddon = fitAddonRef.current;
    const container = containerRef.current;
    if (!terminal || !fitAddon || !container) return;
    if (container.offsetParent === null) return;

    try {
      fitAddon.fit();
      terminal.refresh(0, terminal.rows - 1);
    } catch {
      return;
    }

    if (resize) {
      const { cols, rows } = terminal;
      if (cols > 0 && rows > 0) {
        invoke("pty_resize", { sessionId: ptyId, cols, rows }).catch(() => {});
      }
    }
  };

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const terminal = new XTerm({
      theme: {
        background: "#1e1e2e",
        foreground: "#cdd6f4",
        cursor: "#f5e0dc",
        selectionBackground: "rgba(137, 180, 250, 0.3)",
        black: "#45475a",
        red: "#f38ba8",
        green: "#a6e3a1",
        yellow: "#f9e2af",
        blue: "#89b4fa",
        magenta: "#f5c2e7",
        cyan: "#89dceb",
        white: "#bac2de",
        brightBlack: "#585b70",
        brightRed: "#f38ba8",
        brightGreen: "#a6e3a1",
        brightYellow: "#f9e2af",
        brightBlue: "#89b4fa",
        brightMagenta: "#f5c2e7",
        brightCyan: "#89dceb",
        brightWhite: "#cdd6f4",
      },
      fontFamily: 'Menlo, Monaco, "Courier New", monospace',
      fontSize: 13,
      cursorBlink: true,
      convertEol: true,
      allowTransparency: true,
      linkHandler: {
        activate: async (_event: MouseEvent, uri: string) => {
          await openUrl(uri);
        },
        hover: () => {},
        leave: () => {},
      },
    });

    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new ClipboardAddon());
    terminal.loadAddon(
      new WebLinksAddon(async (_event, uri) => {
        await openUrl(uri);
      }),
    );

    terminal.open(container);

    xtermRef.current = terminal;
    fitAddonRef.current = fitAddon;

    registerTerminal(ptyId, {
      onOutput: (data) => {
        terminal.write(data);
        extendProcess();
      },
      onExit: () => {
        endProcess();
        const { sessions } = useTerminalStore.getState();
        const session = sessions.find((s) => s.id === sessionId);
        if (session && shouldNotify()) {
          notify({
            title: "Terminal finished",
            body: ` "${session.title}" has finished.`,
            sessionId,
          });
        }
        useTerminalStore.getState().removeSession(sessionId).catch(() => {});
      },
    });
    registerTerminalIdle(ptyId, (title) => {
      handleIdle(title);
    });
    registerTerminalBusy(ptyId, () => {
      handleBusy();
    });
    registerTerminalError(ptyId, (message) => {
      endProcess();
      const sanitized = message.replace(/\x1b/g, "");
      terminal.write(`\r\n\x1b[31m✗ ${sanitized}\x1b[0m\r\n`);
    });

    const handleInput = (data: string) => {
      const bytes = new TextEncoder().encode(data);
      let binary = "";
      for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]);
      }
      const base64 = btoa(binary);
      invoke("pty_write", { sessionId: ptyId, data: base64 }).catch(() => {});
      if (data === "\r" || data.includes("\n")) {
        startProcess();
      }
    };
    const dataDisposable = terminal.onData(handleInput);
    const binaryDisposable = terminal.onBinary(handleInput);

    terminal.attachCustomKeyEventHandler((event: KeyboardEvent) => {
      if (event.type !== "keydown") return true;
      if (event.key.toLowerCase() !== "c") return true;
      if (!event.ctrlKey && !event.metaKey) return true;
      if (!terminal.hasSelection()) return true;
      event.preventDefault();
      const selected = terminal.getSelection();
      navigator.clipboard.writeText(selected).catch(() => {});
      terminal.clearSelection();
      return false;
    });

    const resizeObserver = new ResizeObserver(() => fitAndResize(true));
    resizeObserver.observe(container);

    rafRef.current = requestAnimationFrame(() => fitAndResize(true));

    if (isFocused) {
      terminal.focus();
    }

    return () => {
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
      }
      if (processTimeoutRef.current) {
        clearTimeout(processTimeoutRef.current);
      }
      unregisterTerminal(ptyId);
      resizeObserver.disconnect();
      dataDisposable.dispose();
      binaryDisposable.dispose();
      terminal.dispose();
      xtermRef.current = null;
      fitAddonRef.current = null;
    };
  }, [sessionId, ptyId]);

  useEffect(() => {
    if (!isVisible) return;
    const id = requestAnimationFrame(() => fitAndResize(true));
    return () => {
      if (id) cancelAnimationFrame(id);
    };
  }, [isVisible]);

  const [showProcesses, setShowProcesses] = useState(false);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [processesError, setProcessesError] = useState<string | null>(null);
  const [processesLoading, setProcessesLoading] = useState(false);

  useEffect(() => {
    // Start fresh when switching to a different session.
    setShowProcesses(false);
    setProcesses([]);
    setProcessesError(null);
  }, [sessionId]);

  useEffect(() => {
    if (!showProcesses || !isVisible) {
      setProcesses([]);
      setProcessesError(null);
      return;
    }
    let cancelled = false;
    const poll = async () => {
      if (cancelled) return;
      setProcessesLoading(true);
      try {
        const procs = await fetchSessionProcesses(ptyId);
        if (!cancelled) {
          setProcesses(procs);
          setProcessesError(null);
        }
      } catch (e) {
        if (!cancelled) setProcessesError(String(e));
      } finally {
        if (!cancelled) setProcessesLoading(false);
      }
    };
    void poll();
    const id = setInterval(poll, 1000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [showProcesses, isVisible, ptyId]);

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex h-7 shrink-0 items-center gap-1 border-b border-[var(--color-surface0)] px-2">
        <button
          onClick={() => setShowProcesses((v) => !v)}
          className={`flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[10px] ${
            showProcesses
              ? "bg-[var(--color-surface0)] text-[var(--color-blue)]"
              : "text-[var(--color-overlay0)] hover:bg-[var(--color-surface0)] hover:text-[var(--color-text)]"
          }`}
          title="Show processes in this terminal"
        >
          <ListTree size={11} />
          Processes
        </button>
        <span className="text-[10px] text-[var(--color-overlay0)]">
          {processes.length > 0 ? `${processes.length}` : ""}
        </span>
        {showProcesses && (
          <button
            onClick={() => setShowProcesses(false)}
            className="shrink-0 text-[var(--color-overlay1)] hover:text-[var(--color-text)]"
            title="Close processes panel"
          >
            <X size={11} />
          </button>
        )}
      </div>
      <div className="relative flex-1 overflow-hidden flex">
        <div
          ref={containerRef}
          className={`h-full min-w-0 flex-1 ${isVisible ? "" : "hidden"}`}
        />
        {showProcesses && (
          <div className="h-full w-72 shrink-0 border-l border-[var(--color-surface0)] bg-[var(--color-mantle)] overflow-y-auto">
            <div className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
              Processes
            </div>
            {processesLoading && processes.length === 0 && (
              <div className="px-2 py-1 text-[10px] text-[var(--color-overlay0)]">
                Loading...
              </div>
            )}
            {processesError && (
              <div className="px-2 py-1 text-[10px] text-[var(--color-red)]">
                {processesError}
              </div>
            )}
            {!processesLoading && processes.length === 0 && !processesError && (
              <div className="px-2 py-1 text-[10px] text-[var(--color-overlay0)]">
                {isVisible ? "No processes found" : "Terminal hidden"}
              </div>
            )}
            <ul className="flex flex-col gap-0.5 px-2 py-1">
              {processes.map((p) => (
                <li
                  key={p.pid}
                  className="flex items-baseline gap-1.5 rounded-md px-1.5 py-0.5 font-mono text-[10px] text-[var(--color-subtext0)]"
                  title={`pid ${p.pid}\n${p.args || p.comm}`}
                >
                  <span className="shrink-0 text-[9px] text-[var(--color-overlay0)]">
                    {p.pid}
                  </span>
                  <span className="font-semibold text-[var(--color-text)]">
                    {p.comm}
                  </span>
                  {p.args && (
                    <span className="min-w-0 flex-1 truncate text-[var(--color-overlay1)]">
                      {p.args}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
