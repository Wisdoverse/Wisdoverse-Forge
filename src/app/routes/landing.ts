import { useSettingsStore } from '@app/entities/settings'
import { shouldShowGettingStarted } from '@app/shared/lib/gettingStartedPreference'

export type LandingPath = '/start' | '/tasks'

type PreferencesResponse = {
  preferences?: {
    gettingStartedDismissed?: unknown
  }
}

// Post-login landing: first-time users see the setup checklist unless they
// already skipped or completed it. Preference failures still fall through to
// /tasks so setup never blocks work.
export async function resolveLandingPath(): Promise<LandingPath> {
  try {
    const cached = useSettingsStore.getState()
    if (cached.preferencesLoaded) {
      return shouldShowGettingStarted(cached.preferences) ? '/start' : '/tasks'
    }

    const token = localStorage.getItem('af:auth:access')
    if (!token) return '/tasks'
    const response = await fetch('/api/v1/users/me/preferences', {
      headers: { Authorization: `Bearer ${token}` },
    })
    if (!response.ok) return '/tasks'
    const data = (await response.json()) as PreferencesResponse
    return shouldShowGettingStarted(data?.preferences) ? '/start' : '/tasks'
  } catch {
    return '/tasks'
  }
}
