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

function expectBeginnerError(actual: string | null, expected: string): void {
  expect(actual).toBe(expected)
  expect(actual).not.toContain('Code:')
  expect(actual).not.toContain('Details:')
}

beforeEach(() => {
  resetSettingsState()
  Object.values(settingsApiMock).forEach((mock) => mock.mockReset())
  Object.values(agentApiMock).forEach((mock) => mock.mockReset())
})

describe('settingsActionErrorMessage', () => {
  test('turns expired auth into a sign-in step', () => {
    expectBeginnerError(
      settingsActionErrorMessage('providers', 'load', statusError(401, 'HTTP 401')),
      'Sign in again, then open Settings and try to load provider settings again.'
    )
  })

  test('turns permission failures into an admin role step', () => {
    expectBeginnerError(
      settingsActionErrorMessage('apiKeys', 'create', statusError(403, 'Forbidden')),
      'You do not have permission to create the platform access key. Ask an owner or admin to give you access to platform access keys.'
    )
  })

  test('turns platform key validation details into access-key guidance', () => {
    const message = settingsActionErrorMessage(
      'apiKeys',
      'create',
      statusError(422, 'name is required')
    )

    expectBeginnerError(
      message,
      'Name this platform access key, choose the allowed access, then create it again.'
    )
    expect(message).not.toMatch(/A[P]I key/)
  })

  test('turns field validation details into a provider setup step', () => {
    expectBeginnerError(
      settingsActionErrorMessage('providers', 'save', statusError(422, 'model is required')),
      'Choose a supported model for this provider, then save the provider again.'
    )
  })

  test('turns raw network errors into connection guidance', () => {
    expectBeginnerError(
      settingsActionErrorMessage('sshKeys', 'load', 'Network error'),
      'Settings could not load SSH keys because the app could not reach the service. Check your connection and try again.'
    )
  })
})

describe('useSettingsStore errors', () => {
  test('stores beginner guidance when provider loading has a server failure', async () => {
    settingsApiMock.getProviders.mockRejectedValue(
      statusError(503, 'HTTP 503: Service Unavailable')
    )

    await useSettingsStore.getState().loadProviders()

    expectBeginnerError(
      useSettingsStore.getState().providersError,
      'The settings service is temporarily unavailable. Refresh Settings, then try to load provider settings again. If it still fails, ask an owner or admin to check Settings.'
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
    expectBeginnerError(
      useSettingsStore.getState().providersError,
      'Enter the secret key from the model service, choose a model, then save the provider again.'
    )
  })

  test('stores connection guidance when SSH keys cannot load', async () => {
    agentApiMock.getUserSshKeys.mockResolvedValue({ ok: false, keys: [], error: 'Network error' })

    await useSettingsStore.getState().loadSshKeys()

    expectBeginnerError(
      useSettingsStore.getState().sshKeysError,
      'Settings could not load SSH keys because the app could not reach the service. Check your connection and try again.'
    )
  })

  test('turns Git credential configuration details into a setup step', async () => {
    agentApiMock.getGitCredentials.mockResolvedValue({
      ok: false,
      credentials: [],
      error: 'Git provider is not configured',
    })

    await useSettingsStore.getState().loadGitCredentials()

    expectBeginnerError(
      useSettingsStore.getState().gitCredentialsError,
      'Repository access is not configured yet. Ask an owner or admin to configure the Git provider, then refresh repository tokens.'
    )
  })
})
