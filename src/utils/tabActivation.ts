export function nextActiveAfterClose(
  tabIds: string[],
  closedId: string,
  activeId: string | null,
): string | null {
  if (activeId !== closedId) return activeId;
  const idx = tabIds.indexOf(closedId);
  const remaining = tabIds.filter((id) => id !== closedId);
  return remaining[idx] ?? remaining[idx - 1] ?? null;
}
