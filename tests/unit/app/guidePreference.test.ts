import { describe, expect, test } from 'vitest'
import { shouldShowGuide } from '@app/shared/lib/guidePreference'

describe('guidePreference', () => {
  test('hides dismissed guides', () => {
    const preferences = {
      dismissedGuides: ['dismissed'],
      collapsedGuides: ['collapsed'],
    }

    expect(shouldShowGuide('dismissed', preferences)).toBe(false)
    expect(shouldShowGuide('other', preferences)).toBe(true)
  })

  test('treats missing or malformed preference arrays as unset', () => {
    expect(shouldShowGuide('guide', { dismissedGuides: 'guide' })).toBe(true)
  })
})
