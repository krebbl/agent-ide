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
  fetchPrForBranch: (projectId: string, branch: string) => Promise<void>;
  fetchPrsForWorktrees: (
    projectId: string,
    branches: string[],
  ) => Promise<void>;
  getPr: (projectId: string, branch: string) => PrCacheEntry | undefined;
}

export const usePrStore = create<PrStore>((set, get) => ({
  cache: {},

  fetchPrForBranch: async (projectId: string, branch: string) => {
    const key = `${projectId}:${branch}`;
    if (get().cache[key]?.loading) return;
    if (get().cache[key] && !get().cache[key].loading) return;

    set((s) => ({
      cache: { ...s.cache, [key]: { pr: null, loading: true, error: null } },
    }));

    try {
      const result = await prForBranch(projectId, branch);
      set((s) => ({
        cache: {
          ...s.cache,
          [key]: { pr: result.pr, loading: false, error: result.error },
        },
      }));
    } catch (e) {
      set((s) => ({
        cache: {
          ...s.cache,
          [key]: { pr: null, loading: false, error: String(e) },
        },
      }));
    }
  },

  fetchPrsForWorktrees: async (projectId: string, branches: string[]) => {
    console.log("[prStore] fetchPrsForWorktrees", { projectId, branches });
    const toFetch = branches.filter((b) => {
      const key = `${projectId}:${b}`;
      const entry = get().cache[key];
      return !entry || (!entry.loading && entry.pr === null && entry.error === null);
    });

    if (toFetch.length === 0) {
      console.log("[prStore] nothing to fetch (all cached)");
      return;
    }

    console.log("[prStore] fetching", toFetch);

    const entries: Record<string, PrCacheEntry> = {};
    for (const b of toFetch) {
      entries[`${projectId}:${b}`] = { pr: null, loading: true, error: null };
    }
    set((s) => ({ cache: { ...s.cache, ...entries } }));

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

    set((s) => ({ cache: { ...s.cache, ...resolved } }));
    console.log("[prStore] resolved", Object.keys(resolved).map(k => ({ key: k, hasPr: !!resolved[k].pr, error: resolved[k].error })));
  },

  getPr: (projectId: string, branch: string) => {
    return get().cache[`${projectId}:${branch}`];
  },
}));