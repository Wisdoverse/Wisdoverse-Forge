export type GuidePreference = {
  dismissedGuides?: unknown
  collapsedGuides?: unknown
}

function includesGuide(value: unknown, key: string): boolean {
  return Array.isArray(value) && value.includes(key)
}

export function shouldShowGuide(
  key: string,
  preferences: GuidePreference | null | undefined
): boolean {
  return !includesGuide(preferences?.dismissedGuides, key)
}

export function isGuideExpanded(
  key: string,
  preferences: GuidePreference | null | undefined,
  isNewAccount: boolean
): boolean {
  return (
    shouldShowGuide(key, preferences) &&
    isNewAccount &&
    !includesGuide(preferences?.collapsedGuides, key)
  )
}
