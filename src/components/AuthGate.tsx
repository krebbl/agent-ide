import { useState, type FormEvent, type ReactNode } from "react";

const TOKEN_KEY = "agent-ide-token";

export default function AuthGate({ children }: { children: ReactNode }) {
  const [token, setToken] = useState("");

  const isWebMode = import.meta.env.VITE_TAURI !== "true";
  const hasToken = (localStorage.getItem(TOKEN_KEY) ?? "").length > 0;

  if (!isWebMode || hasToken) {
    return <>{children}</>;
  }

  const submit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!token.trim()) return;
    localStorage.setItem(TOKEN_KEY, token.trim());
    window.location.reload();
  };

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
          value={token}
          onChange={(e) => setToken(e.target.value)}
          placeholder="Access token"
          className="mb-4 w-full rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm outline-none focus:border-[var(--color-blue)]"
          autoFocus
        />
        <button
          type="submit"
          className="w-full rounded-md bg-[var(--color-blue)] px-3 py-2 text-sm font-medium text-[var(--color-base)] hover:opacity-90 disabled:opacity-50"
          disabled={!token.trim()}
        >
          Continue
        </button>
      </form>
    </div>
  );
}
