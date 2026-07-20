import { listen } from "@tauri-apps/api/event";

interface PtyOutputEvent {
  sessionId: string;
  data: string;
}

interface PtyExitEvent {
  sessionId: string;
  exitCode: number | null;
}

interface PtyIdleEvent {
  sessionId: string;
  title: string;
}

const outputHandlers = new Map<string, (data: Uint8Array) => void>();
const exitHandlers = new Map<string, () => void>();
const idleHandlers = new Map<string, (title: string) => void>();

function base64ToUint8Array(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

export async function initTerminalEventListeners() {
  await listen<PtyOutputEvent>("pty_output", (event) => {
    const handler = outputHandlers.get(event.payload.sessionId);
    if (handler) {
      handler(base64ToUint8Array(event.payload.data));
    }
  });
  await listen<PtyExitEvent>("pty_exit", (event) => {
    console.log("[terminalEvents] pty_exit received:", event.payload.sessionId);
    const handler = exitHandlers.get(event.payload.sessionId);
    if (handler) {
      handler();
    } else {
      console.warn("[terminalEvents] no exit handler for", event.payload.sessionId);
    }
  });
  await listen<PtyIdleEvent>("pty_idle", (event) => {
    console.log("[terminalEvents] pty_idle received:", event.payload.sessionId);
    const handler = idleHandlers.get(event.payload.sessionId);
    if (handler) {
      handler(event.payload.title);
    } else {
      console.warn("[terminalEvents] no idle handler for", event.payload.sessionId);
    }
  });
}

export function registerTerminal(
  ptyId: string,
  handlers: {
    onOutput: (data: Uint8Array) => void;
    onExit: () => void;
  },
) {
  outputHandlers.set(ptyId, handlers.onOutput);
  exitHandlers.set(ptyId, handlers.onExit);
}

export function unregisterTerminal(ptyId: string) {
  outputHandlers.delete(ptyId);
  exitHandlers.delete(ptyId);
  idleHandlers.delete(ptyId);
}

export function registerTerminalIdle(
  ptyId: string,
  handler: (title: string) => void,
) {
  idleHandlers.set(ptyId, handler);
}
