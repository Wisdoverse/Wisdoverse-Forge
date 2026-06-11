import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { ProvidersSection } from '@app/features/settings/ProvidersSection'
import { useSettingsStore } from '@app/shared/model/settings.store'

const settingsApiMock = vi.hoisted(() => ({
  getSupportedProviders: vi.fn(),
  testProvider: vi.fn(),
}))

vi.mock('@app/shared/api/legacy', () => ({
  getSettingsApi: () => settingsApiMock,
  getAgentApi: vi.fn(),
}))

const loadProvidersMock = vi.fn().mockResolvedValue(undefined)
const saveProviderMock = vi.fn().mockResolvedValue(null)
const deleteProviderMock = vi.fn().mockResolvedValue(true)
const originalLoadProviders = useSettingsStore.getState().loadProviders
const originalSaveProvider = useSettingsStore.getState().saveProvider
const originalDeleteProvider = useSettingsStore.getState().deleteProvider

beforeEach(() => {
  settingsApiMock.getSupportedProviders.mockResolvedValue([])
  settingsApiMock.testProvider.mockResolvedValue({ ok: true, latencyMs: 42 })
  loadProvidersMock.mockClear()
  saveProviderMock.mockClear()
  deleteProviderMock.mockClear()
  useSettingsStore.setState({
    providers: [
      {
        id: 'provider-ready',
        provider: 'openai',
        displayName: 'OpenAI Production',
        model: 'gpt-5.4',
        apiKeyPrefix: 'sk-live',
        priority: 1,
        isEnabled: true,
        isDefault: true,
        lastTestStatus: 'passed',
      },
      {
        id: 'provider-needs-test',
        provider: 'anthropic',
        displayName: 'Anthropic Review',
        model: 'claude-sonnet-4-20250514',
        apiKeyPrefix: 'sk-ant',
        priority: 2,
        isEnabled: true,
        isDefault: false,
        lastTestStatus: 'failed',
        lastTestErrorMessage: 'Invalid key',
      },
      {
        id: 'provider-disabled',
        provider: 'ollama',
        displayName: 'Local Lab',
        model: 'llama3',
        priority: 3,
        isEnabled: false,
        isDefault: false,
        lastTestStatus: 'untested',
      },
    ],
    providersLoading: false,
    providersError: null,
    loadProviders: loadProvidersMock,
    saveProvider: saveProviderMock,
    deleteProvider: deleteProviderMock,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useSettingsStore.setState({
    providers: [],
    providersLoading: false,
    providersError: null,
    loadProviders: originalLoadProviders,
    saveProvider: originalSaveProvider,
    deleteProvider: originalDeleteProvider,
  })
})

describe('ProvidersSection', () => {
  test('summarizes provider readiness and filters providers by action state', async () => {
    render(<ProvidersSection />)

    const readiness = await screen.findByTestId('provider-readiness')
    expect(within(readiness).getByText(/1\/3 providers ready/i)).toBeDefined()
    expect(within(readiness).getByText('Default: OpenAI Production')).toBeDefined()
    const nextStep = screen.getByTestId('provider-next-step')
    expect(within(nextStep).getByText('Do This Next')).toBeDefined()
    expect(within(nextStep).getByText('Test Provider Connection')).toBeDefined()
    expect(screen.getByRole('button', { name: /test openai production connection/i })).toBeDefined()
    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.getByText('Local Lab')).toBeDefined()

    fireEvent.click(within(nextStep).getByRole('button', { name: /show needs test/i }))

    expect(screen.queryByRole('button', { name: /test openai production connection/i })).toBeNull()
    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.queryByRole('button', { name: /test local lab connection/i })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Disabled' }))

    expect(screen.queryByText('Anthropic Review')).toBeNull()
    expect(screen.getByText('Local Lab')).toBeDefined()
  })

  test('searches providers and exposes a clear empty state', async () => {
    render(<ProvidersSection />)

    fireEvent.change(await screen.findByRole('searchbox', { name: /search providers/i }), {
      target: { value: 'review' },
    })

    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.queryByRole('button', { name: /test openai production connection/i })).toBeNull()

    fireEvent.change(screen.getByRole('searchbox', { name: /search providers/i }), {
      target: { value: 'missing-provider' },
    })

    expect(screen.getByText('No providers match this view')).toBeDefined()
  })

  test('guides an empty provider setup into the add form', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    expect(within(nextStep).getByText('Add Your First Provider')).toBeDefined()
    expect(within(nextStep).getByText(/paste the key/i)).toBeDefined()

    fireEvent.click(within(nextStep).getByRole('button', { name: /add provider/i }))

    expect(screen.getByText('Provider setup path')).toBeDefined()
    expect(screen.getByText('Paste key')).toBeDefined()
    expect(screen.getByText(/stored encrypted/i)).toBeDefined()
    expect(screen.getByText('Save, then test')).toBeDefined()
    expect(screen.getByLabelText(/^provider$/i)).toBeDefined()
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(/next: paste api key/i)
    const saveButton = screen.getByRole('button', { name: /save provider/i })
    expect(saveButton).toBeEnabled()

    fireEvent.click(saveButton)

    expect(
      screen.getAllByText('Add the API key before saving this provider.').length
    ).toBeGreaterThan(0)
    expect(saveProviderMock).not.toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: 'sk-test' } })

    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(/ready to save/i)
    fireEvent.click(saveButton)

    await waitFor(() =>
      expect(saveProviderMock).toHaveBeenCalledWith(
        expect.objectContaining({
          provider: 'anthropic',
          model: 'claude-sonnet-4-20250514',
          apiKey: 'sk-test',
        })
      )
    )
  })

  test('does not treat disabled-only providers as ready', async () => {
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-disabled-only',
          provider: 'ollama',
          displayName: 'Local Disabled',
          model: 'llama3',
          priority: 1,
          isEnabled: false,
          isDefault: false,
          lastTestStatus: 'untested',
        },
      ],
    })

    render(<ProvidersSection />)

    const readiness = await screen.findByTestId('provider-readiness')
    expect(within(readiness).getByText('Provider setup needs attention')).toBeDefined()
    const nextStep = screen.getByTestId('provider-next-step')
    expect(within(nextStep).getByText('Add an Active Provider')).toBeDefined()

    fireEvent.click(within(nextStep).getByRole('button', { name: /add provider/i }))

    expect(screen.getByText('Local Disabled')).toBeDefined()
    expect(screen.getByRole('button', { name: /save provider/i })).toBeEnabled()
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(/next: paste api key/i)
  })

  test('surfaces the CN default placeholder and global endpoint hint for region-switch providers', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    fireEvent.click(within(nextStep).getByRole('button', { name: /add provider/i }))

    fireEvent.change(screen.getByLabelText(/^provider$/i), { target: { value: 'zhipu' } })

    // CN endpoint is the default (placeholder); the global endpoint is the hint.
    expect(screen.getByLabelText(/^model$/i)).toHaveValue('glm-4.7')
    expect(screen.getByLabelText(/base url/i)).toHaveAttribute(
      'placeholder',
      expect.stringContaining('https://open.bigmodel.cn/api/paas/v4')
    )
    expect(screen.getByText(/global endpoint: https:\/\/api\.z\.ai\/api\/paas\/v4/i)).toBeDefined()

    // Hunyuan is CN-only — no global endpoint hint, default copy returns.
    fireEvent.change(screen.getByLabelText(/^provider$/i), { target: { value: 'hunyuan' } })
    expect(screen.getByText(/only change this for a local model server/i)).toBeDefined()
    expect(screen.queryByText(/global endpoint:/i)).toBeNull()
  })

  test('runs a provider test from the provider row', async () => {
    render(<ProvidersSection />)

    fireEvent.click(
      await screen.findByRole('button', { name: /test anthropic review connection/i })
    )

    await waitFor(() =>
      expect(settingsApiMock.testProvider).toHaveBeenCalledWith('provider-needs-test')
    )
    expect(loadProvidersMock).toHaveBeenCalled()
  })
})
