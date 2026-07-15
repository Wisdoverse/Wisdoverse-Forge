import { describe, expect, test } from 'vitest'
import { isGuideExpanded, shouldShowGuide } from '@app/shared/lib/guidePreference'

describe('guidePreference', () => {
  test('hides dismissed guides and expands only new-account guides that were not collapsed', () => {
    const preferences = {
      dismissedGuides: ['dismissed'],
      collapsedGuides: ['collapsed'],
    }

    expect(shouldShowGuide('dismissed', preferences)).toBe(false)
    expect(shouldShowGuide('other', preferences)).toBe(true)
    expect(isGuideExpanded('other', preferences, true)).toBe(true)
    expect(isGuideExpanded('collapsed', preferences, true)).toBe(false)
    expect(isGuideExpanded('other', preferences, false)).toBe(false)
    expect(isGuideExpanded('dismissed', preferences, true)).toBe(false)
  })

  test('treats missing or malformed preference arrays as unset', () => {
    expect(shouldShowGuide('guide', { dismissedGuides: 'guide' })).toBe(true)
    expect(isGuideExpanded('guide', { collapsedGuides: 'guide' }, true)).toBe(true)
  })
})
