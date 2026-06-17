import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { resolveLandingPath } from '@app/routes/landing'

const originalFetch = globalThis.fetch

function resetPreferencesState() {
  useSettingsStore.setState({
    preferences: null,
    preferencesLoaded: false,
    preferencesLoading: false,
  })
}

beforeEach(() => {
  resetPreferencesState()
  localStorage.clear()
  globalThis.fetch = vi.fn()
})

afterEach(() => {
  resetPreferencesState()
  localStorage.clear()
  globalThis.fetch = originalFetch
  vi.restoreAllMocks()
})

describe('landing route preference', () => {
  test('sends users who skipped or completed Start to Tasks from cached preferences', async () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
    })

    await expect(resolveLandingPath()).resolves.toBe('/tasks')
    expect(globalThis.fetch).not.toHaveBeenCalled()
  })

  test('keeps first-time users on Start from cached preferences', async () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: false },
      preferencesLoaded: true,
    })

    await expect(resolveLandingPath()).resolves.toBe('/start')
    expect(globalThis.fetch).not.toHaveBeenCalled()
  })

  test('checks saved preferences on a fresh page load before choosing Start', async () => {
    localStorage.setItem('af:auth:access', 'token')
    vi.mocked(globalThis.fetch).mockResolvedValue({
      ok: true,
      json: async () => ({ preferences: { gettingStartedDismissed: true } }),
    } as Response)

    await expect(resolveLandingPath()).resolves.toBe('/tasks')
    expect(globalThis.fetch).toHaveBeenCalledWith('/api/v1/users/me/preferences', {
      headers: { Authorization: 'Bearer token' },
    })
  })

  test('falls back to Start when preferences cannot be read', async () => {
    localStorage.setItem('af:auth:access', 'token')
    vi.mocked(globalThis.fetch).mockRejectedValue(new Error('offline'))

    await expect(resolveLandingPath()).resolves.toBe('/start')
  })
})
