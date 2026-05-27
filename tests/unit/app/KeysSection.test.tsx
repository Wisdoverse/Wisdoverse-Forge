import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { KeysSection } from '@app/features/settings/KeysSection'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { ApiKeyRecord } from '@app/shared/api/legacy/settingsApi'

const loadApiKeysMock = vi.fn().mockResolvedValue(undefined)
const createApiKeyMock = vi.fn().mockResolvedValue(null)
const revokeApiKeyMock = vi.fn().mockResolvedValue(true)

const originalLoadApiKeys = useSettingsStore.getState().loadApiKeys
const originalCreateApiKey = useSettingsStore.getState().createApiKey
const originalRevokeApiKey = useSettingsStore.getState().revokeApiKey

function apiKey(overrides: Partial<ApiKeyRecord> = {}): ApiKeyRecord {
  return {
    id: 'key-1',
    name: 'Deploy pipeline',
    keyPrefix: 'af_live',
    createdAt: '2026-05-06T07:00:00.000Z',
    lastUsedAt: null,
    ...overrides,
  }
}

beforeEach(() => {
  loadApiKeysMock.mockClear()
  createApiKeyMock.mockReset().mockResolvedValue(null)
  revokeApiKeyMock.mockClear()
  useSettingsStore.setState({
    apiKeys: [],
    keysLoading: false,
    keysError: null,
    loadApiKeys: loadApiKeysMock,
    createApiKey: createApiKeyMock,
    revokeApiKey: revokeApiKeyMock,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useSettingsStore.setState({
    apiKeys: [],
    keysLoading: false,
    keysError: null,
    loadApiKeys: originalLoadApiKeys,
    createApiKey: originalCreateApiKey,
    revokeApiKey: originalRevokeApiKey,
  })
})

describe('KeysSection', () => {
  test('guides an empty API key setup into the create form', async () => {
    render(<KeysSection />)

    await waitFor(() => expect(loadApiKeysMock).toHaveBeenCalledTimes(1))
    const emptyState = screen.getByTestId('platform-key-empty-state')

    expect(within(emptyState).getByText('No platform API keys yet')).toBeDefined()
    expect(within(emptyState).getByText(/another tool needs to call Forge/i)).toBeDefined()
    expect(within(emptyState).getByText(/trusted script, CI job, or integration/i)).toBeDefined()
    expect(within(emptyState).getByText(/password manager or CI secret/i)).toBeDefined()

    fireEvent.click(within(emptyState).getByRole('button', { name: /create platform key/i }))

    expect(screen.getByLabelText(/^key name$/i)).toBeDefined()
    expect(screen.getByText(/Name the exact place this key will be used/i)).toBeDefined()
  })

  test('creates a key and shows one-time save guidance', async () => {
    createApiKeyMock.mockResolvedValue({
      key: 'af_test_key_value',
      apiKey: apiKey({ name: 'Production deploy pipeline' }),
    })

    render(<KeysSection />)

    await waitFor(() => expect(loadApiKeysMock).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getAllByRole('button', { name: /create platform key/i })[0])
    fireEvent.change(screen.getByLabelText(/^key name$/i), {
      target: { value: 'Production deploy pipeline' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create$/i }))

    await waitFor(() => expect(createApiKeyMock).toHaveBeenCalledWith('Production deploy pipeline'))
    expect(screen.getByText(/copy it now/i)).toBeDefined()
    expect(screen.getByText(/password manager or CI secret store/i)).toBeDefined()
    expect(screen.getByText('af_test_key_value')).toBeDefined()
  })
})
