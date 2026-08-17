const TOKEN_KEY = "agent-ide-token";

type UnauthorizedListener = () => void;

const listeners = new Set<UnauthorizedListener>();

export function getToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? "";
}

export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

export function onUnauthorized(listener: UnauthorizedListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function emitUnauthorized(): void {
  if (!getToken()) return;
  clearToken();
  listeners.forEach((listener) => listener());
}
