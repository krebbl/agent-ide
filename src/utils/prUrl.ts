import { PrInfo } from "../types";

export function prChangesUrl(pr: PrInfo): string | null {
  if (!pr.url.startsWith("http")) return null;
  return pr.provider === "bitbucket" ? `${pr.url}/diff` : `${pr.url}/files`;
}
