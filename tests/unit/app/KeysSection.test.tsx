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

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
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

    expect(screen.getByRole('heading', { name: 'Outside tool access' })).toBeDefined()
    expect(within(emptyState).getByText('Add a key only for a trusted outside tool')).toBeDefined()
    expect(within(emptyState).getAllByText(/trusted outside tool/i).length).toBeGreaterThan(0)
    expect(within(emptyState).getByText(/skip this until a trusted outside tool/i)).toBeDefined()
    expect(within(emptyState).getByText(/tool you trust/i)).toBeDefined()
    expect(within(emptyState).getByText(/access value in a password manager/i)).toBeDefined()
    expect(within(emptyState).queryByText(/copy the new key/i)).toBeNull()
    expect(within(emptyState).queryByText('No outside tool access keys yet')).toBeNull()
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
    expect(screen.getByText(/Outside tool access key created - save this value now/i)).toBeDefined()
    expect(screen.getByText(/full access value is shown only once/i)).toBeDefined()
    expect(screen.queryByText(/full key is shown/i)).toBeNull()
    expect(screen.getByRole('button', { name: /copy access value/i })).toBeDefined()
    expect(screen.queryByRole('button', { name: /copy key/i })).toBeNull()
    const savedButton = screen.getByRole('button', { name: /i saved this value/i })
    expect(savedButton).toBeDefined()
    expect(screen.getByText('af_test_key_value')).toBeDefined()

    fireEvent.click(savedButton)

    expect(screen.getByText('af_test_key_value')).toBeDefined()
    expect(screen.getByText(/This value disappears after you hide it/i)).toBeDefined()
    expect(screen.getByText(/Save it in a password manager first/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /hide saved value now/i })).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /hide saved value now/i }))

    expect(screen.queryByText('af_test_key_value')).toBeNull()
  })

  test('shows manual save guidance when copying the one-time key fails', async () => {
    createApiKeyMock.mockResolvedValue({
      key: 'af_test_key_value',
      apiKey: apiKey({ name: 'Production deploy pipeline' }),
    })
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) },
    })

    render(<KeysSection />)

    await waitFor(() => expect(loadApiKeysMock).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getAllByRole('button', { name: /create access key/i })[0])
    fireEvent.change(screen.getByLabelText(/^which tool will use this key/i), {
      target: { value: 'Production deploy pipeline' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create access key/i }))
    await waitFor(() => expect(createApiKeyMock).toHaveBeenCalledWith('Production deploy pipeline'))

    fireEvent.click(screen.getByRole('button', { name: /copy access value/i }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Select the access value text, then copy it manually before choosing I saved this value.'
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/clipboard access/i)
  })

  test('labels saved key rows with clear removal language', async () => {
    useSettingsStore.setState({
      apiKeys: [apiKey({ name: 'Release automation', keyPrefix: 'af_rel' })],
    })

    render(<KeysSection />)

    expect(await screen.findByRole('table', { name: /outside tool access keys/i })).toBeDefined()
    expect(screen.getByText('Saved key starts with')).toBeDefined()
    expect(screen.getByText('Use this key from a trusted tool first')).toBeDefined()
    expect(screen.queryByText('Not used yet')).toBeNull()
    expect(screen.queryByText('Starts with')).toBeNull()
    expect(screen.queryByText('Key preview')).toBeNull()
    expect(screen.queryByText('—')).toBeNull()

    fireEvent.click(
      screen.getByRole('button', {
        name: /remove outside tool access key named release automation/i,
      })
    )

    expect(revokeApiKeyMock).not.toHaveBeenCalled()
    expect(
      screen.getByText('Removing this key can stop Release automation from connecting to Forge.')
    ).toBeDefined()
    expect(screen.getByRole('button', { name: /^keep key$/i })).toBeDefined()
    expect(
      screen.getByRole('button', {
        name: /confirm removing outside tool access key named release automation/i,
      })
    ).toHaveTextContent('Remove now')

    fireEvent.click(screen.getByRole('button', { name: /^keep key$/i }))
    expect(revokeApiKeyMock).not.toHaveBeenCalled()
    expect(screen.queryByRole('button', { name: /^keep key$/i })).toBeNull()

    fireEvent.click(
      screen.getByRole('button', {
        name: /remove outside tool access key named release automation/i,
      })
    )

    fireEvent.click(
      screen.getByRole('button', {
        name: /confirm removing outside tool access key named release automation/i,
      })
    )

    expect(revokeApiKeyMock).toHaveBeenCalledWith('key-1')
  })

  test('locks removal controls while an access key is being removed', async () => {
    const request = deferred<boolean>()
    revokeApiKeyMock.mockReturnValueOnce(request.promise)
    useSettingsStore.setState({
      apiKeys: [apiKey({ name: 'Release automation', keyPrefix: 'af_rel' })],
    })

    render(<KeysSection />)

    expect(await screen.findByRole('table', { name: /outside tool access keys/i })).toBeDefined()
    fireEvent.click(
      screen.getByRole('button', {
        name: /remove outside tool access key named release automation/i,
      })
    )
    fireEvent.click(
      screen.getByRole('button', {
        name: /confirm removing outside tool access key named release automation/i,
      })
    )

    const removingButton = await screen.findByRole('button', { name: /removing/i })
    expect(removingButton).toBeDisabled()
    expect(removingButton).toHaveAttribute('aria-busy', 'true')
    expect(screen.getByRole('button', { name: /^keep key$/i })).toBeDisabled()
    expect(revokeApiKeyMock).toHaveBeenCalledTimes(1)

    request.resolve(true)
    await waitFor(() => expect(screen.queryByRole('button', { name: /^keep key$/i })).toBeNull())
  })

  test('explains missing access key dates instead of showing placeholders', async () => {
    useSettingsStore.setState({
      apiKeys: [
        apiKey({
          name: 'Scheduled report export',
          createdAt: '',
          lastUsedAt: 'not-a-date',
        }),
      ],
    })

    render(<KeysSection />)

    expect(await screen.findByRole('table', { name: /outside tool access keys/i })).toBeDefined()
    expect(screen.getByText('Refresh access keys to load created date')).toBeDefined()
    expect(screen.getByText('Refresh access keys to check last use')).toBeDefined()
    expect(screen.queryByText('Invalid Date')).toBeNull()
    expect(screen.queryByText('—')).toBeNull()
  })

  test('shows a beginner recovery step instead of raw platform key details', async () => {
    useSettingsStore.setState({
      keysError:
        'You do not have permission to create the platform API key. Code: 403. Details: Forbidden',
    })

    render(<KeysSection />)

    await waitFor(() => expect(loadApiKeysMock).toHaveBeenCalledTimes(1))
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Ask an owner or admin to let you create or remove outside tool access keys.'
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(
      'Outside tool access key could not be created.'
    )
    expect(screen.queryByText(/Details: Forbidden/i)).toBeNull()
  })
})
