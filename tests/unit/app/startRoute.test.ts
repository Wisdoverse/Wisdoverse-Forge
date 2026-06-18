import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { skipDismissedStartRoute } from '@app/routes/start'

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

describe('Start route preference', () => {
  test('skips Start when the setup checklist is already hidden', async () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
    })

    await expect(skipDismissedStartRoute()).rejects.toMatchObject({ options: { to: '/tasks' } })
  })

  test('keeps Start available when the setup checklist is visible', async () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: false },
      preferencesLoaded: true,
    })

    await expect(skipDismissedStartRoute()).resolves.toBeUndefined()
    expect(globalThis.fetch).not.toHaveBeenCalled()
  })

  test('skips Start when no restore preference exists yet', async () => {
    useSettingsStore.setState({
      preferences: {},
      preferencesLoaded: true,
    })

    await expect(skipDismissedStartRoute()).rejects.toMatchObject({ options: { to: '/tasks' } })
  })
})
