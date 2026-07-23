import { listen } from "@tauri-apps/api/event";
import { useTerminalStore } from "../stores/terminalStore";

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

interface PtyBusyEvent {
  sessionId: string;
  title: string;
}

interface PtyStateSnapshotEvent {
  sessionId: string;
  isBusy: boolean;
  title: string;
}

const outputHandlers = new Map<string, (data: Uint8Array) => void>();
const exitHandlers = new Map<string, () => void>();
const idleHandlers = new Map<string, (title: string) => void>();
const busyHandlers = new Map<string, (title: string) => void>();

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
    const handler = exitHandlers.get(event.payload.sessionId);
    if (handler) {
      handler();
    } else {
      useTerminalStore.getState().removeSession(
        useTerminalStore.getState().sessions.find((s) => s.ptyId === event.payload.sessionId)?.id ?? ""
      ).catch(() => {});
    }
  });
  await listen<PtyIdleEvent>("pty_idle", (event) => {
    const store = useTerminalStore.getState();
    const session = store.sessions.find((s) => s.ptyId === event.payload.sessionId);
    const wasBusy = session?.isBusy === true || session?.processRunning === true;
    const isNotActive = session?.id !== store.activeSessionId;
    store.updateSessionByPtyId(event.payload.sessionId, {
      isBusy: false,
      needsInput: true,
      title: event.payload.title,
      ...(wasBusy && isNotActive ? { hasUnseenActivity: true } : {}),
    });
    const handler = idleHandlers.get(event.payload.sessionId);
    if (handler) {
      handler(event.payload.title);
    }
  });
  await listen<PtyBusyEvent>("pty_busy", (event) => {
    useTerminalStore.getState().updateSessionByPtyId(event.payload.sessionId, {
      isBusy: true,
      needsInput: false,
      title: event.payload.title,
    });
    const handler = busyHandlers.get(event.payload.sessionId);
    if (handler) {
      handler(event.payload.title);
    }
  });
  await listen<PtyStateSnapshotEvent>("pty_state_snapshot", (event) => {
    useTerminalStore.getState().updateSessionByPtyId(event.payload.sessionId, {
      isBusy: event.payload.isBusy,
      needsInput: !event.payload.isBusy,
      title: event.payload.title,
    });
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
  busyHandlers.delete(ptyId);
}

export function registerTerminalIdle(
  ptyId: string,
  handler: (title: string) => void,
) {
  idleHandlers.set(ptyId, handler);
}

export function registerTerminalBusy(
  ptyId: string,
  handler: (title: string) => void,
) {
  busyHandlers.set(ptyId, handler);
}
