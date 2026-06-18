export type GettingStartedPreference = {
  gettingStartedDismissed?: unknown
}

export function shouldShowGettingStarted(
  preferences: GettingStartedPreference | null | undefined
): boolean {
  return preferences?.gettingStartedDismissed === false
}
