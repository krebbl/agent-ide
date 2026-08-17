import type { IpcEvent } from "../ipc";

const TOKEN_KEY = "agent-ide-token";

function getToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? "";
}

export async function invoke<T>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  const token = getToken();
  const response = await fetch(`/-/invoke/${command}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(payload ?? {}),
  });

  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw String(text || response.statusText);
  }

  const text = await response.text();
  return (text ? (JSON.parse(text) as T) : undefined) as T;
}

interface ServerEventMessage {
  event: string;
  payload: unknown;
}

const handlers = new Map<string, Set<(event: IpcEvent<unknown>) => void>>();

let ws: WebSocket | null = null;
let reconnectDelay = 1000;
let reconnectTimer: number | null = null;
let intentionalClose = false;
let connecting = false;

function eventUrl(): string {
  const token = getToken();
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/-/events${token ? `?token=${encodeURIComponent(token)}` : ""}`;
}

function connect() {
  if (connecting || (ws && ws.readyState !== WebSocket.CLOSED)) return;
  connecting = true;
  intentionalClose = false;

  const socket = new WebSocket(eventUrl());
  ws = socket;

  socket.addEventListener("open", () => {
    reconnectDelay = 1000;
    connecting = false;
  });

  socket.addEventListener("message", (event) => {
    let data: ServerEventMessage | null = null;
    try {
      data = JSON.parse(event.data) as ServerEventMessage;
    } catch {
      return;
    }
    if (!data || typeof data.event !== "string") return;
    const eventHandlers = handlers.get(data.event);
    if (!eventHandlers) return;
    for (const handler of eventHandlers) {
      handler({ payload: data!.payload });
    }
  });

  socket.addEventListener("close", () => {
    connecting = false;
    ws = null;
    if (!intentionalClose) {
      scheduleReconnect();
    }
  });

  socket.addEventListener("error", () => {
    connecting = false;
  });
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    reconnectDelay = Math.min(reconnectDelay * 2, 8000);
    connect();
  }, reconnectDelay);
}

function ensureConnected() {
  if (!ws || ws.readyState === WebSocket.CLOSED) {
    connect();
  }
}

export function listen<T>(
  event: string,
  handler: (event: IpcEvent<T>) => void,
): Promise<() => void> {
  if (!handlers.has(event)) {
    handlers.set(event, new Set());
  }
  const set = handlers.get(event)!;
  const wrapped = (e: IpcEvent<unknown>) => handler(e as IpcEvent<T>);
  set.add(wrapped);
  ensureConnected();

  return Promise.resolve(() => {
    set.delete(wrapped);
    if (set.size === 0) {
      handlers.delete(event);
    }
  });
}
