import type { ContextPreviewItem } from '@shared/types/context'

/** Trim the selection until it fits at most this fraction of the budget. */
export const CONTEXT_TRIM_TARGET_RATIO = 0.85

/**
 * Recommend which selected context items to remove so the agent's window
 * fits: pinned items are protected, least-recently-used items go first,
 * then the largest ones, and the selection never empties completely.
 * Returns an empty list when the selection already fits (or cannot be
 * trimmed further).
 */
export function suggestContextTrims(
  items: ContextPreviewItem[],
  selectedIds: Set<string>,
  budget: number,
  pinnedIds: Set<string>,
  targetRatio = CONTEXT_TRIM_TARGET_RATIO
): string[] {
  if (budget <= 0) return []
  const selected = items.filter((item) => selectedIds.has(item.id))
  let selectedTokens = selected.reduce((sum, item) => {
    const tokens = Number.isFinite(item.estimatedTokens) ? item.estimatedTokens : 0
    return sum + tokens
  }, 0)
  if (selectedTokens <= budget * targetRatio) return []

  const candidates = selected
    .filter((item) => !pinnedIds.has(item.id))
    .map((item) => ({
      id: item.id,
      lastUsedAt: item.lastUsedAt ? Date.parse(item.lastUsedAt) : 0,
      tokens: Number.isFinite(item.estimatedTokens) ? item.estimatedTokens : 0,
    }))
    .sort((a, b) => {
      if (a.lastUsedAt !== b.lastUsedAt) return a.lastUsedAt - b.lastUsedAt
      return b.tokens - a.tokens
    })

  const remove: string[] = []
  for (const candidate of candidates) {
    if (selectedIds.size - remove.length <= 1) break
    if (selectedTokens <= budget * targetRatio) break
    selectedTokens -= candidate.tokens
    remove.push(candidate.id)
  }
  return remove
}
