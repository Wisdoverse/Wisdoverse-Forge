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
      'Sign in again, then open Settings and try to load AI service settings again.'
    )
  })

  test('turns permission failures into an admin role step', () => {
    expectBeginnerError(
      settingsActionErrorMessage('apiKeys', 'create', statusError(403, 'Forbidden')),
      'Ask an owner or admin to give you access to outside tool access keys, then try to create the outside tool access key again. You do not have permission to create the outside tool access key.'
    )
  })

  test('turns structured permission failures into an admin role step', () => {
    const message = settingsActionErrorMessage('apiKeys', 'create', {
      detail: 'outside tool policy denied',
      status: '403',
    })

    expectBeginnerError(
      message,
      'Ask an owner or admin to give you access to outside tool access keys, then try to create the outside tool access key again. You do not have permission to create the outside tool access key.'
    )
    expect(message).not.toContain('policy denied')
  })

  test('turns platform key validation details into access-key guidance', () => {
    const message = settingsActionErrorMessage(
      'apiKeys',
      'create',
      statusError(422, 'name is required')
    )

    expectBeginnerError(
      message,
      'Name this outside tool access key, choose the allowed access, then create it again.'
    )
    expect(message).not.toMatch(/A[P]I key/)
  })

  test('uses Settings API server details for provider field guidance', () => {
    const message = settingsActionErrorMessage(
      'providers',
      'save',
      Object.assign(new Error('HTTP 422: Unprocessable Entity'), {
        serverError: 'base url is required',
        statusCode: 422,
      })
    )

    expectBeginnerError(message, 'Add the service address for this AI service, then save again.')
    expect(message).not.toContain('Unprocessable')
    expect(message).not.toContain('base url is required')
  })

  test('turns field validation details into a provider setup step', () => {
    expectBeginnerError(
      settingsActionErrorMessage('providers', 'save', statusError(422, 'model is required')),
      'Keep the suggested model or choose a supported model, then save again.'
    )
  })

  test('turns general provider validation into AI service setup guidance', () => {
    expectBeginnerError(
      settingsActionErrorMessage('providers', 'save', statusError(422, 'invalid provider')),
      'Choose an AI service from the list, then save again.'
    )
  })

  test('turns GitHub or GitLab site validation into a site choice step', () => {
    expectBeginnerError(
      settingsActionErrorMessage('gitCredentials', 'save', statusError(422, 'invalid provider')),
      'Choose GitHub or GitLab, then save code access again.'
    )
  })

  test('turns GitHub or GitLab address validation into an address step', () => {
    expectBeginnerError(
      settingsActionErrorMessage('gitCredentials', 'save', statusError(422, 'invalid host')),
      'Check the GitHub or GitLab address. Leave it blank for github.com or gitlab.com, then save again.'
    )
  })

  test('turns git@ Repository Access label validation into a naming step', () => {
    expectBeginnerError(
      settingsActionErrorMessage('sshKeys', 'create', statusError(422, 'label is required')),
      'Add a name for this git@ Repository Access, then save again.'
    )
  })

  test('turns public key validation into shareable-line guidance', () => {
    expectBeginnerError(
      settingsActionErrorMessage('sshKeys', 'create', statusError(422, 'public key is invalid')),
      'Paste the public key line that starts with ssh-ed25519 or ssh-rsa, then save again.'
    )
  })

  test('turns raw network errors into connection guidance', () => {
    const message = settingsActionErrorMessage('sshKeys', 'load', 'Network error')

    expectBeginnerError(
      message,
      'Check your connection, then refresh Settings to load git@ Repository Access. Forge could not connect while loading Settings.'
    )
    expect(message).not.toContain('SSH keys')
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('service')
  })

  test('turns structured connection failures into connection guidance', () => {
    const message = settingsActionErrorMessage('providers', 'load', {
      detail: 'connection refused by settings gateway',
    })

    expectBeginnerError(
      message,
      'Check your connection, then refresh Settings to load AI service settings. Forge could not connect while loading Settings.'
    )
    expect(message).not.toContain('connection refused')
    expect(message).not.toContain('gateway')
  })

  test('starts update connection failures with the recovery step', () => {
    const message = settingsActionErrorMessage('providers', 'save', 'Network error')

    expectBeginnerError(
      message,
      'Check your connection, then try to save the AI service again. Forge could not connect while updating Settings.'
    )
    expect(message).not.toContain('Network error')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    const message = settingsActionErrorMessage('runtime', 'update', {
      code: '429',
      reason: 'too many runtime writes',
    })

    expectBeginnerError(
      message,
      'The Settings page is busy. Wait a moment, then try to update agent work settings again.'
    )
    expect(message).not.toContain('runtime writes')
  })

  test('starts Settings server failures with the retry step', () => {
    const message = settingsActionErrorMessage(
      'providers',
      'load',
      statusError(503, 'HTTP 503: Service Unavailable')
    )

    expectBeginnerError(
      message,
      'Refresh Settings, then try to load AI service settings again. Forge could not load Settings right now. If it still fails, ask an owner or admin to check Settings.'
    )
    expect(message).not.toContain('HTTP 503')
    expect(message).not.toContain('Service Unavailable')
  })

  test('starts unknown Settings failures with the retry step', () => {
    expectBeginnerError(
      settingsActionErrorMessage('providers', 'load', statusError(418, 'teapot')),
      'Refresh Settings, then try to load AI service settings again. Settings could not load AI service settings.'
    )
  })

  test('uses product labels for code access permission errors', () => {
    const message = settingsActionErrorMessage(
      'gitCredentials',
      'save',
      statusError(403, 'HTTP 403')
    )

    expectBeginnerError(
      message,
      'Ask an owner or admin to let you manage code access, then try to save the code access again. You do not have permission to save the code access.'
    )
    expect(message).not.toContain('Git credential')
    expect(message).not.toContain('Git credentials')
  })

  test('uses product labels for work capacity validation errors', () => {
    const message = settingsActionErrorMessage(
      'resourceProfiles',
      'load',
      statusError(422, 'profile missing')
    )

    expectBeginnerError(
      message,
      'Ask an owner or admin to add an agent size, then refresh Settings.'
    )
    expect(message).not.toContain('resource profile')
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
      'Refresh Settings, then try to load AI service settings again. Forge could not load Settings right now. If it still fails, ask an owner or admin to check Settings.'
    )
    expect(useSettingsStore.getState().providersError).not.toContain('provider settings')
    expect(useSettingsStore.getState().providersError).not.toContain('HTTP 503')
    expect(useSettingsStore.getState().providersError).not.toContain('temporarily unavailable')
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
      'Paste the service access key from the selected AI service, then save again.'
    )
  })

  test('stores connection guidance when SSH keys cannot load', async () => {
    agentApiMock.getUserSshKeys.mockResolvedValue({ ok: false, keys: [], error: 'Network error' })

    await useSettingsStore.getState().loadSshKeys()

    expectBeginnerError(
      useSettingsStore.getState().sshKeysError,
      'Check your connection, then refresh Settings to load git@ Repository Access. Forge could not connect while loading Settings.'
    )
    expect(useSettingsStore.getState().sshKeysError).not.toContain('Network error')
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
      'Code access is not configured yet. Ask an owner or admin to finish GitHub or GitLab setup, then refresh code access.'
    )
  })
})
