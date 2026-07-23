import { create } from "zustand";
import { PrInfo } from "../types";
import { prForBranch } from "../services/prInfo";

type PrCacheEntry = {
  pr: PrInfo | null;
  loading: boolean;
  error: string | null;
};

interface PrStore {
  cache: Record<string, PrCacheEntry>;
  tick: number;
  lastFetchedAt: Record<string, number>;
  fetchPrForBranch: (projectId: string, branch: string) => Promise<void>;
  fetchPrsForWorktrees: (
    projectId: string,
    branches: string[],
    force?: boolean,
  ) => Promise<void>;
  getPr: (projectId: string, branch: string) => PrCacheEntry | undefined;
}

export const usePrStore = create<PrStore>((set, get) => ({
  cache: {},
  tick: 0,
  lastFetchedAt: {},

  fetchPrForBranch: async (projectId: string, branch: string) => {
    const key = `${projectId}:${branch}`;
    if (get().cache[key]?.loading) return;
    if (get().cache[key] && !get().cache[key].loading) return;

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

    try {
      const result = await prForBranch(projectId, branch);
      set((s) => ({
        cache: {
          ...s.cache,
          [key]: { pr: result.pr, loading: false, error: result.error },
        },
        tick: s.tick + 1,
      }));
    } catch (e) {
      set((s) => ({
        cache: {
          ...s.cache,
          [key]: { pr: null, loading: false, error: String(e) },
        },
        tick: s.tick + 1,
      }));
    }
  },

  fetchPrsForWorktrees: async (projectId: string, branches: string[], force = false) => {
    const toFetch = branches.filter((b) => {
      if (force) return true;
      const key = `${projectId}:${b}`;
      const entry = get().cache[key];
      return !entry || entry.error !== null || (!entry.loading && entry.pr === null && entry.error === null);
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
    });

    set((s) => ({ cache: { ...s.cache, ...resolved }, tick: s.tick + 1 }));
  },

  getPr: (projectId: string, branch: string) => {
    return get().cache[`${projectId}:${branch}`];
  },
}));