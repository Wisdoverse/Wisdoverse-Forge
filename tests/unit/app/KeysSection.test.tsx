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
  test('guides an empty access key setup into the create form', async () => {
    render(<KeysSection />)

    await waitFor(() => expect(loadApiKeysMock).toHaveBeenCalledTimes(1))
    const emptyState = screen.getByTestId('platform-key-empty-state')

    expect(screen.getByRole('heading', { name: 'Automation access keys' })).toBeDefined()
    expect(within(emptyState).getByText('No automation access keys yet')).toBeDefined()
    expect(within(emptyState).getByText(/trusted outside tool/i)).toBeDefined()
    expect(within(emptyState).getByText(/tool you trust/i)).toBeDefined()
    expect(
      within(emptyState).getByText(/password manager before closing this message/i)
    ).toBeDefined()
    expect(within(emptyState).queryByText(/platform A[P]I keys/i)).toBeNull()

    fireEvent.click(within(emptyState).getByRole('button', { name: /create access key/i }))

    expect(screen.queryByTestId('platform-key-empty-state')).toBeNull()
    expect(screen.getByLabelText(/^which tool will use this key/i)).toBeDefined()
    expect(screen.getByText(/use a clear tool or job name/i)).toBeDefined()
    expect(screen.getByText(/remove the right key later/i)).toBeDefined()
  })

  test('explains the required key name before creating an access key', async () => {
    render(<KeysSection />)

    await waitFor(() => expect(loadApiKeysMock).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getAllByRole('button', { name: /create access key/i })[0])

    const input = screen.getByLabelText(/^which tool will use this key/i)
    const form = input.closest('form')
    expect(form).toBeTruthy()

    screen.getByRole('button', { name: /cancel/i }).focus()
    fireEvent.submit(form!)

    expect(createApiKeyMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      /name the tool that will use this access key first/i
    )
    expect(input).toHaveFocus()
  })

  test('creates a key and shows one-time save guidance', async () => {
    createApiKeyMock.mockResolvedValue({
      key: 'af_test_key_value',
      apiKey: apiKey({ name: 'Production deploy pipeline' }),
    })

    render(<KeysSection />)

    await waitFor(() => expect(loadApiKeysMock).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getAllByRole('button', { name: /create access key/i })[0])
    fireEvent.change(screen.getByLabelText(/^which tool will use this key/i), {
      target: { value: 'Production deploy pipeline' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create access key/i }))

    await waitFor(() => expect(createApiKeyMock).toHaveBeenCalledWith('Production deploy pipeline'))
    expect(screen.getByText(/Automation access key created - save it now/i)).toBeDefined()
    expect(screen.getByText(/only time the full key is shown/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /copy key/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /i saved it/i })).toBeDefined()
    expect(screen.getByText('af_test_key_value')).toBeDefined()
  })

  test('labels saved key rows with clear removal language', async () => {
    useSettingsStore.setState({
      apiKeys: [apiKey({ name: 'Release automation', keyPrefix: 'af_rel' })],
    })

    render(<KeysSection />)

    expect(await screen.findByRole('table', { name: /automation access keys/i })).toBeDefined()
    expect(screen.getByText('Starts with')).toBeDefined()

    fireEvent.click(
      screen.getByRole('button', { name: /remove automation access key named release automation/i })
    )

    expect(
      screen.getByRole('button', {
        name: /confirm removing automation access key named release automation/i,
      })
    ).toHaveTextContent('Remove now')
  })

  test('shows a beginner recovery step instead of raw platform key details', async () => {
    useSettingsStore.setState({
      keysError:
        'You do not have permission to create the platform API key. Code: 403. Details: Forbidden',
    })

    render(<KeysSection />)

    await waitFor(() => expect(loadApiKeysMock).toHaveBeenCalledTimes(1))
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Automation access key could not be created. Ask an owner or admin to let you create or remove automation access keys.'
    )
    expect(screen.queryByText(/Details: Forbidden/i)).toBeNull()
  })
})
