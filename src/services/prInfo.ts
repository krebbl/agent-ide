import { invoke } from "@tauri-apps/api/core";
import { PrInfo, PrInfoResult } from "../types";

export async function prForBranch(
  projectId: string,
  branch: string,
): Promise<PrInfoResult> {
  return await invoke<PrInfoResult>("pr_for_branch", { projectId, branch });
}

export async function prListForRepo(projectId: string): Promise<PrInfo[]> {
  return await invoke<PrInfo[]>("pr_list_for_repo", { projectId });
}