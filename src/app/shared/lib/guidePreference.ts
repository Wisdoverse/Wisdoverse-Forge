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
