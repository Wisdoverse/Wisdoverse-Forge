import { useSettingsStore } from '@app/shared/model/settings.store'

export type LandingPath = '/start' | '/tasks'

type PreferencesResponse = {
  preferences?: {
    gettingStartedDismissed?: unknown
  }
}

// Post-login landing: first-time users get the Getting Started checklist,
// users who skipped or completed it land on the task board. Any preferences
// failure falls back to /start so landing never blocks on this request.
export async function resolveLandingPath(): Promise<LandingPath> {
  try {
    const cached = useSettingsStore.getState()
    if (cached.preferencesLoaded) {
      return cached.preferences?.gettingStartedDismissed === true ? '/tasks' : '/start'
    }

    const token = localStorage.getItem('af:auth:access')
    if (!token) return '/start'
    const response = await fetch('/api/v1/users/me/preferences', {
      headers: { Authorization: `Bearer ${token}` },
    })
    if (!response.ok) return '/start'
    const data = (await response.json()) as PreferencesResponse
    return data?.preferences?.gettingStartedDismissed === true ? '/tasks' : '/start'
  } catch {
    return '/start'
  }
}
