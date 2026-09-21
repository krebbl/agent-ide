import { create } from "zustand";
import { PrComment, PrInfo } from "../types";
import { prForBranch, prThreadsForBranch } from "../services/prInfo";

type PrCacheEntry = {
  pr: PrInfo | null;
  loading: boolean;
  error: string | null;
};

export type PrThreadsEntry = {
  comments: PrComment[];
  viewerLogin: string | null;
  loading: boolean;
  error: string | null;
};

export const NO_PR_TTL_MS = 5 * 60_000;

// Positive PR results older than this are refetched on the next poll;
// fresher entries survive force-refreshes (polling, window focus).
export const PR_TTL_MS = 2 * 60_000;

// Transient errors (rate limit, network) are retried at most once per interval.
export const ERROR_RETRY_MS = 30_000;

export function isNoPrError(error: string | null | undefined): boolean {
  return !!error && /no pull requests?\b|no pr\b/i.test(error);
}

interface PrStore {
  cache: Record<string, PrCacheEntry>;
  threads: Record<string, PrThreadsEntry>;
  tick: number;
  lastFetchedAt: Record<string, number>;
  noPrAt: Record<string, number>;
  fetchedAt: Record<string, number>;
  fetchPrForBranch: (projectId: string, branch: string, force?: boolean) => Promise<void>;
  fetchThreads: (projectId: string, branch: string, prNumber: string, force?: boolean) => Promise<void>;
  fetchPrsForWorktrees: (
    projectId: string,
    branches: string[],
    force?: boolean,
  ) => Promise<void>;
  getPr: (projectId: string, branch: string) => PrCacheEntry | undefined;
}

export const usePrStore = create<PrStore>((set, get) => ({
  cache: {},
  threads: {},
  tick: 0,
  lastFetchedAt: {},
  noPrAt: {},
  fetchedAt: {},

  fetchPrForBranch: async (projectId: string, branch: string, force = false) => {
    const key = `${projectId}:${branch}`;
    if (!force) {
      if (get().cache[key]?.loading) return;
      if (get().cache[key] && !get().cache[key].loading) return;
    }

    set((s) => {
      const existing = s.cache[key];
      return {
        cache: {
          ...s.cache,
          [key]: existing
            ? { ...existing, loading: true }
            : { pr: null, loading: true, error: null },
        },
      };
    });

    const stamp = (s: { fetchedAt: Record<string, number> }) => ({
      ...s.fetchedAt,
      [key]: Date.now(),
    });

    try {
      const result = await prForBranch(projectId, branch);
      set((s) => ({
        cache: {
          ...s.cache,
          [key]: { pr: result.pr, loading: false, error: result.error },
        },
        noPrAt:
          result.pr === null && isNoPrError(result.error)
            ? { ...s.noPrAt, [key]: Date.now() }
            : s.noPrAt,
        fetchedAt: stamp(s),
        tick: s.tick + 1,
      }));
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      set((s) => ({
        cache: {
          ...s.cache,
          [key]: { pr: null, loading: false, error: message },
        },
        noPrAt: isNoPrError(message) ? { ...s.noPrAt, [key]: Date.now() } : s.noPrAt,
        fetchedAt: stamp(s),
        tick: s.tick + 1,
      }));
    }
  },

  fetchPrsForWorktrees: async (projectId: string, branches: string[], force = false) => {
    const toFetch = branches.filter((b) => {
      const key = `${projectId}:${b}`;
      const entry = get().cache[key];
      if (entry?.loading) return false;
      // Known to have no PR: negative-cache for a TTL so polling stops re-querying it.
      if (entry && entry.pr === null && isNoPrError(entry.error)) {
        return Date.now() - (get().noPrAt[key] ?? 0) > NO_PR_TTL_MS;
      }
      // Fresh positive result: keep it even across force-refreshes.
      if (
        force &&
        entry &&
        entry.pr !== null &&
        Date.now() - (get().fetchedAt[key] ?? 0) < PR_TTL_MS
      ) {
        return false;
      }
      // Transient errors: back off regardless of force (poll interval is force=true).
      if (entry && entry.error !== null) {
        return Date.now() - (get().fetchedAt[key] ?? 0) > ERROR_RETRY_MS;
      }
      if (force) return true;
      if (!entry) return true;
      return !entry.loading && entry.pr === null && entry.error === null;
    });

    if (toFetch.length === 0) return;

    set((s) => {
      const entries: Record<string, PrCacheEntry> = {};
      for (const b of toFetch) {
        const key = `${projectId}:${b}`;
        const existing = s.cache[key];
        entries[key] = existing
          ? { ...existing, loading: true }
          : { pr: null, loading: true, error: null };
      }
      return {
        cache: { ...s.cache, ...entries },
        tick: s.tick + 1,
        lastFetchedAt: { ...s.lastFetchedAt, [projectId]: Date.now() },
      };
    });

    const results = await Promise.allSettled(
      toFetch.map((b) => prForBranch(projectId, b)),
    );

    const resolved: Record<string, PrCacheEntry> = {};
    const noPrKeys: string[] = [];
    toFetch.forEach((b, i) => {
      const key = `${projectId}:${b}`;
      const r = results[i];
      if (r.status === "fulfilled") {
        resolved[key] = {
          pr: r.value.pr,
          loading: false,
          error: r.value.error,
        };
      } else {
        resolved[key] = {
          pr: null,
          loading: false,
          error: String(r.reason),
        };
      }
      if (resolved[key].pr === null && isNoPrError(resolved[key].error)) {
        noPrKeys.push(key);
      }
    });

    set((s) => {
      const noPrAt = { ...s.noPrAt };
      for (const key of noPrKeys) noPrAt[key] = Date.now();
      const fetchedAt = { ...s.fetchedAt };
      for (const b of toFetch) fetchedAt[`${projectId}:${b}`] = Date.now();
      return { cache: { ...s.cache, ...resolved }, tick: s.tick + 1, noPrAt, fetchedAt };
    });
  },

  getPr: (projectId: string, branch: string) => {
    return get().cache[`${projectId}:${branch}`];
  },

  fetchThreads: async (projectId: string, branch: string, prNumber: string, force = false) => {
    const key = `${projectId}:${branch}`;
    const existing = get().threads[key];
    if (!force && (existing?.loading || existing)) return;

    set((s) => ({
      threads: {
        ...s.threads,
        [key]: existing
          ? { ...existing, loading: true, error: null }
          : { comments: [], viewerLogin: null, loading: true, error: null },
      },
    }));

    try {
      const result = await prThreadsForBranch(projectId, prNumber);
      set((s) => ({
        threads: {
          ...s.threads,
          [key]: {
            comments: result.comments,
            viewerLogin: result.viewerLogin,
            loading: false,
            error: result.error,
          },
        },
        tick: s.tick + 1,
      }));
    } catch (e) {
      set((s) => ({
        threads: {
          ...s.threads,
          [key]: {
            comments: [],
            viewerLogin: null,
            loading: false,
            error: e instanceof Error ? e.message : String(e),
          },
        },
        tick: s.tick + 1,
      }));
    }
  },
}));