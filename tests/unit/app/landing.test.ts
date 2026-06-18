import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { resolveLandingPath } from '@app/routes/landing'
import { shouldShowGettingStarted } from '@app/shared/lib/gettingStartedPreference'

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
  test('only shows Start after the user restores the setup checklist', () => {
    expect(shouldShowGettingStarted({ gettingStartedDismissed: false })).toBe(true)
    expect(shouldShowGettingStarted({ gettingStartedDismissed: true })).toBe(false)
    expect(shouldShowGettingStarted({})).toBe(false)
    expect(shouldShowGettingStarted(null)).toBe(false)
  })

  test('sends users who skipped or completed Start to Tasks from cached preferences', async () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
    })

    await expect(resolveLandingPath()).resolves.toBe('/tasks')
    expect(globalThis.fetch).not.toHaveBeenCalled()
  })

  test('keeps restored Start available from cached preferences', async () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: false },
      preferencesLoaded: true,
    })

    await expect(resolveLandingPath()).resolves.toBe('/start')
    expect(globalThis.fetch).not.toHaveBeenCalled()
  })

  test('sends users with no Start preference to Tasks', async () => {
    useSettingsStore.setState({
      preferences: {},
      preferencesLoaded: true,
    })

    await expect(resolveLandingPath()).resolves.toBe('/tasks')
    expect(globalThis.fetch).not.toHaveBeenCalled()
  })

  test('checks saved preferences on a fresh page load before choosing Tasks', async () => {
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

  test('keeps restored Start available after a fresh preference read', async () => {
    localStorage.setItem('af:auth:access', 'token')
    vi.mocked(globalThis.fetch).mockResolvedValue({
      ok: true,
      json: async () => ({ preferences: { gettingStartedDismissed: false } }),
    } as Response)

    await expect(resolveLandingPath()).resolves.toBe('/start')
  })

  test('falls back to Tasks when preferences cannot be read', async () => {
    localStorage.setItem('af:auth:access', 'token')
    vi.mocked(globalThis.fetch).mockRejectedValue(new Error('offline'))

    await expect(resolveLandingPath()).resolves.toBe('/tasks')
  })
})
