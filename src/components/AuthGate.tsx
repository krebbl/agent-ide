import { useCallback, useEffect, useState, type ReactNode } from "react";
import { getToken, onUnauthorized, setToken } from "../services/auth";
import { invoke } from "../services/ipc";

const VALIDATE_INTERVAL_MS = 30000;

function isUnauthorizedError(err: unknown): boolean {
  const text = String(err).toLowerCase();
  return text.includes("unauthorized") || text.includes("401");
}

export default function AuthGate({ children }: { children: ReactNode }) {
  const isWebMode = import.meta.env.VITE_TAURI !== "true";
  const [storedToken, setStoredToken] = useState(() => getToken());
  const [inputToken, setInputToken] = useState("");
  const [needsAuth, setNeedsAuth] = useState(() => isWebMode && storedToken.length === 0);

  const validate = useCallback(async () => {
    if (!isWebMode) {
      setNeedsAuth(false);
      return;
    }
    if (!storedToken) {
      setNeedsAuth(true);
      return;
    }
    try {
      const valid = await invoke<boolean>("validate_token", { token: storedToken });
      setNeedsAuth(!valid);
    } catch (err) {
      if (isUnauthorizedError(err)) {
        setNeedsAuth(true);
      }
    }
  }, [isWebMode, storedToken]);

  useEffect(() => {
    validate();
    if (!isWebMode) return;
    const id = setInterval(validate, VALIDATE_INTERVAL_MS);
    return () => clearInterval(id);
  }, [isWebMode, validate]);

  useEffect(() => {
    if (!isWebMode) return;
    return onUnauthorized(() => {
      setStoredToken("");
      setNeedsAuth(true);
    });
  }, [isWebMode]);

  const submit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const trimmed = inputToken.trim();
    if (!trimmed) return;
    setToken(trimmed);
    window.location.reload();
  };

  if (!needsAuth) {
    return <>{children}</>;
  }

  return (
    <div className="flex h-screen w-screen items-center justify-center bg-[var(--color-base)] text-[var(--color-text)]">
      <form
        onSubmit={submit}
        className="w-80 rounded-lg border border-[var(--color-surface1)] bg-[var(--color-mantle)] p-6 shadow-lg"
      >
        <h1 className="mb-1 text-lg font-semibold">Agent IDE</h1>
        <p className="mb-4 text-sm text-[var(--color-subtext0)]">
          Enter your server access token to continue.
        </p>
        <input
          type="password"
          value={inputToken}
          onChange={(e) => setInputToken(e.target.value)}
          placeholder="Access token"
          className="mb-4 w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm outline-none focus:border-[var(--color-blue)]"
          autoFocus
        />
        <button
          type="submit"
          className="w-full rounded-md bg-[var(--color-blue)] px-3 py-2 text-sm font-medium text-[var(--color-base)] hover:opacity-90 disabled:opacity-50"
          disabled={!inputToken.trim()}
        >
          Continue
        </button>
      </form>
    </div>
  );
}
