import { beforeEach, describe, expect, test, vi } from 'vitest'

const settingsApiMock = vi.hoisted(() => ({
  getProviders: vi.fn(),
  createProvider: vi.fn(),
}))

const agentApiMock = vi.hoisted(() => ({
  getGitCredentials: vi.fn(),
  getUserSshKeys: vi.fn(),
}))

vi.mock('@app/shared/api/legacy', () => ({
  getSettingsApi: () => settingsApiMock,
  getAgentApi: () => agentApiMock,
}))

import { settingsActionErrorMessage, useSettingsStore } from '@app/shared/model/settings.store'

function resetSettingsState() {
  useSettingsStore.setState({
    providers: [],
    providersLoading: false,
    providersError: null,
    apiKeys: [],
    keysLoading: false,
    keysError: null,
    gitCredentials: [],
    gitCredentialsLoading: false,
    gitCredentialsError: null,
    sshKeys: [],
    sshKeysLoading: false,
    sshKeysError: null,
    resourceProfiles: [],
    resourceProfilesLoading: false,
    resourceProfilesError: null,
    runtimeSettings: null,
    runtimeLoading: false,
    runtimeError: null,
  })
}

function statusError(statusCode: number, message: string): Error & { statusCode: number } {
  return Object.assign(new Error(message), { statusCode })
}

beforeEach(() => {
  resetSettingsState()
  Object.values(settingsApiMock).forEach((mock) => mock.mockReset())
  Object.values(agentApiMock).forEach((mock) => mock.mockReset())
})

describe('settingsActionErrorMessage', () => {
  test('turns expired auth into a sign-in step', () => {
    expect(settingsActionErrorMessage('providers', 'load', statusError(401, 'HTTP 401'))).toBe(
      'Sign in again, then load provider settings. Code: 401.'
    )
  })

  test('turns permission failures into an admin role step', () => {
    expect(settingsActionErrorMessage('apiKeys', 'create', statusError(403, 'Forbidden'))).toBe(
      'You do not have permission to create the platform API key. Ask an admin to update your role. Code: 403. Details: Forbidden'
    )
  })

  test('keeps field validation details after the operator action', () => {
    expect(
      settingsActionErrorMessage('providers', 'save', statusError(422, 'model is required'))
    ).toBe(
      'Check the required fields for provider, then try again. Code: 422. Details: model is required'
    )
  })

  test('turns raw network errors into connection guidance', () => {
    expect(settingsActionErrorMessage('sshKeys', 'load', 'Network error')).toBe(
      'Settings could not load SSH keys because the browser could not reach the server. Check your connection and try again.'
    )
  })
})

describe('useSettingsStore errors', () => {
  test('stores beginner guidance when provider loading has a server failure', async () => {
    settingsApiMock.getProviders.mockRejectedValue(
      statusError(503, 'HTTP 503: Service Unavailable')
    )

    await useSettingsStore.getState().loadProviders()

    expect(useSettingsStore.getState().providersError).toBe(
      'The settings service had a server problem. Try again after the backend is healthy. Code: 503.'
    )
  })

  test('stores validation guidance when provider creation fails', async () => {
    settingsApiMock.createProvider.mockRejectedValue(statusError(422, 'API key is required'))

    const result = await useSettingsStore.getState().saveProvider({
      provider: 'openai',
      displayName: 'OpenAI',
      model: 'gpt-4o',
      apiKey: '',
    })

    expect(result).toBeNull()
    expect(useSettingsStore.getState().providersError).toBe(
      'Check the required fields for provider, then try again. Code: 422. Details: API key is required'
    )
  })

  test('stores connection guidance when SSH keys cannot load', async () => {
    agentApiMock.getUserSshKeys.mockResolvedValue({ ok: false, keys: [], error: 'Network error' })

    await useSettingsStore.getState().loadSshKeys()

    expect(useSettingsStore.getState().sshKeysError).toBe(
      'Settings could not load SSH keys because the browser could not reach the server. Check your connection and try again.'
    )
  })

  test('keeps useful git credential details when the API returns one', async () => {
    agentApiMock.getGitCredentials.mockResolvedValue({
      ok: false,
      credentials: [],
      error: 'Git provider is not configured',
    })

    await useSettingsStore.getState().loadGitCredentials()

    expect(useSettingsStore.getState().gitCredentialsError).toBe(
      'Settings could not load Git credentials. Review the message and try again. Details: Git provider is not configured'
    )
  })
})
