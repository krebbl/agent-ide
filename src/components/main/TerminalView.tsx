import { useEffect, useRef } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { invoke } from "@tauri-apps/api/core";
import {
  registerTerminal,
  unregisterTerminal,
} from "../../services/terminalEvents";
import { useTerminalStore } from "../../stores/terminalStore";
import "@xterm/xterm/css/xterm.css";

interface TerminalViewProps {
  sessionId: string;
  ptyId: string;
  isActive: boolean;
}

export default function TerminalView({
  sessionId,
  ptyId,
  isActive,
}: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const rafRef = useRef<number | null>(null);
  const busyTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const markBusy = () => {
    useTerminalStore.getState().setSessionBusy(sessionId, true);
    if (busyTimeoutRef.current) {
      clearTimeout(busyTimeoutRef.current);
    }
    busyTimeoutRef.current = setTimeout(() => {
      useTerminalStore.getState().setSessionBusy(sessionId, false);
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
    });

    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new WebLinksAddon());
    terminal.open(container);

    xtermRef.current = terminal;
    fitAddonRef.current = fitAddon;

    registerTerminal(ptyId, {
      onOutput: (data) => {
        terminal.write(data);
        markBusy();
      },
      onExit: () => useTerminalStore.getState().removeSession(sessionId).catch(() => {}),
    });

    const handleInput = (data: string) => {
      invoke("pty_write", { sessionId: ptyId, data }).catch(() => {});
    };
    const dataDisposable = terminal.onData(handleInput);
    const binaryDisposable = terminal.onBinary(handleInput);

    const resizeObserver = new ResizeObserver(() => fitAndResize(true));
    resizeObserver.observe(container);

    rafRef.current = requestAnimationFrame(() => fitAndResize(true));

    return () => {
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
      }
      if (busyTimeoutRef.current) {
        clearTimeout(busyTimeoutRef.current);
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
    if (!isActive) return;
    const id = requestAnimationFrame(() => fitAndResize(true));
    return () => {
      if (id) cancelAnimationFrame(id);
    };
  }, [isActive]);

  return (
    <div
      ref={containerRef}
      className={`absolute inset-0 h-full w-full ${isActive ? "" : "hidden"}`}
    />
  );
}
