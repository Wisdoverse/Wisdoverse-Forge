import { beforeEach, describe, expect, test, vi } from 'vitest'

const agentApiMock = vi.hoisted(() => ({
  getUserPreferences: vi.fn(),
  updateUserPreferences: vi.fn(),
}))

vi.mock('@app/shared/api/legacy', () => ({
  getSettingsApi: () => ({}),
  getAgentApi: () => agentApiMock,
}))

import { useSettingsStore } from '@app/shared/model/settings.store'

function resetPreferencesState() {
  useSettingsStore.setState({
    preferences: null,
    preferencesLoaded: false,
    preferencesLoading: false,
  })
}

beforeEach(() => {
  resetPreferencesState()
  Object.values(agentApiMock).forEach((mock) => mock.mockReset())
})

describe('settings store user preferences', () => {
  test('loadPreferences stores the server document and marks it loaded', async () => {
    agentApiMock.getUserPreferences.mockResolvedValue({
      ok: true,
      preferences: { defaultCliTool: 'codex', gettingStartedDismissed: true },
    })

    await useSettingsStore.getState().loadPreferences()

    const state = useSettingsStore.getState()
    expect(state.preferences).toEqual({ defaultCliTool: 'codex', gettingStartedDismissed: true })
    expect(state.preferencesLoaded).toBe(true)
    expect(state.preferencesLoading).toBe(false)
  })

  test('loadPreferences keeps the default state on failure so callers can retry', async () => {
    agentApiMock.getUserPreferences.mockResolvedValue({ ok: false, preferences: {} })

    await useSettingsStore.getState().loadPreferences()

    const state = useSettingsStore.getState()
    expect(state.preferences).toBeNull()
    expect(state.preferencesLoaded).toBe(false)
    expect(state.preferencesLoading).toBe(false)

    // A later call retries the request instead of staying wedged.
    agentApiMock.getUserPreferences.mockResolvedValue({ ok: true, preferences: {} })
    await useSettingsStore.getState().loadPreferences()
    expect(useSettingsStore.getState().preferencesLoaded).toBe(true)
    expect(agentApiMock.getUserPreferences).toHaveBeenCalledTimes(2)
  })

  test('loadPreferences does not refetch after a successful load', async () => {
    agentApiMock.getUserPreferences.mockResolvedValue({ ok: true, preferences: {} })

    await useSettingsStore.getState().loadPreferences()
    await useSettingsStore.getState().loadPreferences()

    expect(agentApiMock.getUserPreferences).toHaveBeenCalledTimes(1)
  })

  test('loadPreferences survives an uninitialised API client', async () => {
    agentApiMock.getUserPreferences.mockImplementation(() => {
      throw new Error('AgentAPI not initialised — call initLegacyApis() first')
    })

    await expect(useSettingsStore.getState().loadPreferences()).resolves.toBeUndefined()

    const state = useSettingsStore.getState()
    expect(state.preferencesLoaded).toBe(false)
    expect(state.preferencesLoading).toBe(false)
  })

  test('setGettingStartedDismissed patches the server and applies its merged document', async () => {
    useSettingsStore.setState({ preferences: {}, preferencesLoaded: true })
    agentApiMock.updateUserPreferences.mockResolvedValue({
      ok: true,
      preferences: { defaultCliTool: 'codex', gettingStartedDismissed: true },
    })

    const ok = await useSettingsStore.getState().setGettingStartedDismissed(true)

    expect(ok).toBe(true)
    expect(agentApiMock.updateUserPreferences).toHaveBeenCalledWith({
      gettingStartedDismissed: true,
    })
    // The server's merged document wins, preserving keys the patch never sent.
    expect(useSettingsStore.getState().preferences).toEqual({
      defaultCliTool: 'codex',
      gettingStartedDismissed: true,
    })
  })

  test('setGettingStartedDismissed applies an optimistic update before the request resolves', async () => {
    useSettingsStore.setState({ preferences: {}, preferencesLoaded: true })
    let resolveRequest: (value: unknown) => void = () => undefined
    agentApiMock.updateUserPreferences.mockReturnValue(
      new Promise((resolve) => {
        resolveRequest = resolve
      })
    )

    const pending = useSettingsStore.getState().setGettingStartedDismissed(true)

    expect(useSettingsStore.getState().preferences?.gettingStartedDismissed).toBe(true)

    resolveRequest({ ok: true, preferences: { gettingStartedDismissed: true } })
    await expect(pending).resolves.toBe(true)
  })

  test('setGettingStartedDismissed reverts the optimistic update when the patch fails', async () => {
    useSettingsStore.setState({
      preferences: { defaultCliTool: 'claude' },
      preferencesLoaded: true,
    })
    agentApiMock.updateUserPreferences.mockResolvedValue({ ok: false, preferences: {} })

    const ok = await useSettingsStore.getState().setGettingStartedDismissed(true)

    expect(ok).toBe(false)
    expect(useSettingsStore.getState().preferences).toEqual({ defaultCliTool: 'claude' })
  })
})
