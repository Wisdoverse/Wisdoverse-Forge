import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { KeysSection } from '@app/features/settings'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { CreateApiKeyResult } from '@app/shared/api/legacy/settingsApi'

const createdKey: CreateApiKeyResult = {
  key: 'forge_key_example',
  apiKey: {
    id: 'key-1',
    orgId: 'org-1',
    userId: 'user-1',
    name: 'CI job',
    keyPrefix: 'forge_',
    createdAt: '2026-05-24T08:00:00Z',
    lastUsedAt: null,
    expiresAt: null,
  },
}

const loadApiKeysMock = vi.fn().mockResolvedValue(undefined)
const createApiKeyMock = vi.fn().mockResolvedValue(createdKey)
const revokeApiKeyMock = vi.fn().mockResolvedValue(true)
const originalLoadApiKeys = useSettingsStore.getState().loadApiKeys
const originalCreateApiKey = useSettingsStore.getState().createApiKey
const originalRevokeApiKey = useSettingsStore.getState().revokeApiKey

beforeEach(() => {
  loadApiKeysMock.mockClear()
  createApiKeyMock.mockClear()
  revokeApiKeyMock.mockClear()
  loadApiKeysMock.mockResolvedValue(undefined)
  createApiKeyMock.mockResolvedValue(createdKey)
  revokeApiKeyMock.mockResolvedValue(true)
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
  test('keeps platform key creation actionable and explains a missing name', async () => {
    render(<KeysSection />)

    fireEvent.click(screen.getByRole('button', { name: /create platform key/i }))

    const status = screen.getByTestId('platform-key-form-status')
    expect(within(status).getByText('Next: Name the Platform Key')).toBeInTheDocument()
    const createButton = screen.getByRole('button', { name: /^create$/i })
    expect(createButton).not.toBeDisabled()

    fireEvent.click(createButton)

    expect(createApiKeyMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Name this platform API key before creating it.'
    )
    const nameInput = screen.getByLabelText(/key name/i)
    expect(nameInput).toHaveFocus()

    fireEvent.change(nameInput, { target: { value: ' CI job ' } })

    expect(within(status).getByText('Ready to Create Key')).toBeInTheDocument()
    fireEvent.click(createButton)

    await waitFor(() => expect(createApiKeyMock).toHaveBeenCalledWith('CI job'))
    await waitFor(() => expect(screen.getByText('forge_key_example')).toBeInTheDocument())
  })
})
