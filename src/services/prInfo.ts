import { invoke } from "./ipc";
import { PrInfo, PrInfoResult, PrThreadsResult } from "../types";

export async function prForBranch(
  projectId: string,
  branch: string,
): Promise<PrInfoResult> {
  return await invoke<PrInfoResult>("pr_for_branch", { projectId, branch });
}

export async function prListForRepo(projectId: string): Promise<PrInfo[]> {
  return await invoke<PrInfo[]>("pr_list_for_repo", { projectId });
}

export async function prThreadsForBranch(
  projectId: string,
  prNumber: string,
): Promise<PrThreadsResult> {
  return await invoke<PrThreadsResult>("pr_threads_for_branch", { projectId, prNumber });
}