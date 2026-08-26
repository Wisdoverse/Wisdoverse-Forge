import { describe, expect, test } from 'vitest'
import { suggestContextTrims } from '@app/entities/context/model/contextTrim'
import type { ContextPreviewItem } from '@shared/types/context'

function item(id: string, tokens: number, lastUsedAt: string | null = null, pinnedField = false): ContextPreviewItem {
  return {
    id,
    itemKind: 'memory' as const,
    title: id,
    selected: true,
    pinned: pinnedField,
    scopeKind: 'project' as const,
    scopeId: 'scope-1',
    sensitivity: 'internal' as const,
    estimatedTokens: tokens,
    lastUsedAt,
    lastVerifiedAt: null,
    why: 'test',
  }
}

describe('suggestContextTrims', () => {
  test('returns nothing when the selection already fits', () => {
    const items = [item('a', 200), item('b', 100)]
    const result = suggestContextTrims(items, new Set(['a', 'b']), 1000, new Set())
    expect(result).toEqual([])
  })

  test('drops least-recently-used items first, then the largest', () => {
    const stale = item('stale', 300, '2026-05-01T00:00:00Z')
    const fresh = item('fresh', 300, '2026-05-06T00:00:00Z')
    const never = item('never', 100)
    const items = [fresh, stale, never]
    const result = suggestContextTrims(items, new Set(['fresh', 'stale', 'never']), 700, new Set())
    expect(result).toEqual(['never', 'stale'])
  })

  test('never removes pinned items', () => {
    const pinned = item('pinned', 800, null, true)
    const loose = item('loose', 300)
    const items = [pinned, loose]
    const result = suggestContextTrims(items, new Set(['pinned', 'loose']), 900, new Set(['pinned']))
    expect(result).toEqual(['loose'])
  })

  test('keeps at least one selected item even when everything is large', () => {
    const items = [item('big1', 700), item('big2', 700)]
    const result = suggestContextTrims(items, new Set(['big1', 'big2']), 800, new Set())
    expect(result.length).toBe(1)
    expect(result[0]).toBe('big1')
  })

  test('stops trimming once the selection fits the target ratio', () => {
    const items = [item('large', 400), item('small', 100)]
    const result = suggestContextTrims(items, new Set(['large', 'small']), 500, new Set())
    expect(result).toEqual(['large'])
    expect((500 - 100) / 500).toBeLessThanOrEqual(0.85 + 0.0001)
  })
})
