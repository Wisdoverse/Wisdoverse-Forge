import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { KeysSection } from '@app/features/settings/KeysSection'
import { useSettingsStore } from '@app/shared/model/settings.store'

const loadApiKeysMock = vi.fn().mockResolvedValue(undefined)
const createApiKeyMock = vi.fn()
const revokeApiKeyMock = vi.fn().mockResolvedValue(true)
const originalLoadApiKeys = useSettingsStore.getState().loadApiKeys
const originalCreateApiKey = useSettingsStore.getState().createApiKey
const originalRevokeApiKey = useSettingsStore.getState().revokeApiKey

beforeEach(() => {
  loadApiKeysMock.mockClear()
  createApiKeyMock.mockClear()
  createApiKeyMock.mockResolvedValue({
    key: 'af_test_plaintext_key',
    apiKey: {
      id: 'api-key-1',
      name: 'CI deploy',
      keyPrefix: 'af_test',
      createdAt: '2026-05-25T00:00:00.000Z',
      lastUsedAt: null,
    },
  })
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
  test('guides platform API key creation before showing the one-time key', async () => {
    render(<KeysSection />)

    expect(await screen.findByText('No platform API keys yet')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /create platform key/i }))

    expect(screen.getByText('Platform key setup path')).toBeDefined()
    expect(screen.getByText('Name the use')).toBeDefined()
    expect(screen.getByText(/appears only immediately after creation/i)).toBeDefined()
    expect(screen.getByText('Store safely')).toBeDefined()
    expect(screen.getByText(/easy to revoke later/i)).toBeDefined()

    const createButton = screen.getByRole('button', { name: /^create$/i })
    expect(createButton).toBeDisabled()

    fireEvent.change(screen.getByLabelText(/^key name/i), { target: { value: 'CI deploy' } })
    expect(createButton).toBeEnabled()
    fireEvent.click(createButton)

    await waitFor(() => expect(createApiKeyMock).toHaveBeenCalledWith('CI deploy'))
    expect(await screen.findByText(/copy it now/i)).toBeDefined()
    expect(screen.getByText('af_test_plaintext_key')).toBeDefined()
  })
})
