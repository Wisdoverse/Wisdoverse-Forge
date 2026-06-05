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
  test('summarizes model service readiness and filters services by action state', async () => {
    render(<ProvidersSection />)

    const readiness = await screen.findByTestId('provider-readiness')
    expect(within(readiness).getByText(/1\/3 model services ready/i)).toBeDefined()
    expect(within(readiness).getByText('Default service: OpenAI Production')).toBeDefined()
    const nextStep = screen.getByTestId('provider-next-step')
    expect(within(nextStep).getByText('Next step')).toBeDefined()
    expect(within(nextStep).getByText('Check Model Service Connection')).toBeDefined()
    expect(
      screen.getByRole('button', { name: /check openai production connection/i })
    ).toBeDefined()
    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.getByText('Local Lab')).toBeDefined()

    fireEvent.click(within(nextStep).getByRole('button', { name: /show services needing check/i }))

    expect(screen.queryByRole('button', { name: /check openai production connection/i })).toBeNull()
    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.queryByRole('button', { name: /check local lab connection/i })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Disabled' }))

    expect(screen.queryByText('Anthropic Review')).toBeNull()
    expect(screen.getByText('Local Lab')).toBeDefined()
  })

  test('searches model services and exposes a clear empty state', async () => {
    render(<ProvidersSection />)

    fireEvent.change(await screen.findByRole('searchbox', { name: /search model services/i }), {
      target: { value: 'review' },
    })

    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.queryByRole('button', { name: /check openai production connection/i })).toBeNull()

    fireEvent.change(screen.getByRole('searchbox', { name: /search model services/i }), {
      target: { value: 'missing-provider' },
    })

    expect(screen.getByText('No model services match this view')).toBeDefined()
  })

  test('guides an empty model service setup into the add form', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    expect(within(nextStep).getByText('Add Your First Model Service')).toBeDefined()
    expect(within(nextStep).getByText(/confirm the model, add its access key/i)).toBeDefined()

    fireEvent.click(within(nextStep).getByRole('button', { name: /add model service/i }))

    expect(screen.getByText('Model service setup')).toBeDefined()
    expect(screen.getByText('Choose AI service')).toBeDefined()
    expect(screen.getByText('Add service access key')).toBeDefined()
    expect(screen.getByText(/AI services call this an API key/i)).toBeDefined()
    expect(screen.getByText('Save and check')).toBeDefined()
    expect(screen.getByText(/run check before using this service/i)).toBeDefined()
    expect(screen.getByLabelText(/^AI service$/i)).toBeDefined()
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(
      /next: add the service access key/i
    )
    const saveButton = screen.getByRole('button', { name: /save model service/i })
    expect(saveButton).toBeEnabled()

    fireEvent.click(saveButton)

    expect(
      screen.getAllByText('Add the service access key before saving this model service.').length
    ).toBeGreaterThan(0)
    expect(saveProviderMock).not.toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText(/service access key/i), {
      target: { value: 'sk-test' },
    })

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
    expect(within(readiness).getByText('Model service setup needs attention')).toBeDefined()
    const nextStep = screen.getByTestId('provider-next-step')
    expect(within(nextStep).getByText('Add an Active Model Service')).toBeDefined()

    fireEvent.click(within(nextStep).getByRole('button', { name: /add model service/i }))

    expect(screen.getByText('Local Disabled')).toBeDefined()
    expect(screen.getByRole('button', { name: /save model service/i })).toBeEnabled()
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(
      /next: add the service access key/i
    )
  })

  test('runs a connection check from the model service row', async () => {
    render(<ProvidersSection />)

    fireEvent.click(
      await screen.findByRole('button', { name: /check anthropic review connection/i })
    )

    await waitFor(() =>
      expect(settingsApiMock.testProvider).toHaveBeenCalledWith('provider-needs-test')
    )
    expect(loadProvidersMock).toHaveBeenCalled()
  })

  test('labels model service removal with clear confirmation language', async () => {
    render(<ProvidersSection />)

    fireEvent.click(
      await screen.findByRole('button', { name: /remove anthropic review model service/i })
    )

    expect(
      screen.getByRole('button', {
        name: /confirm removing anthropic review model service/i,
      })
    ).toHaveTextContent('Remove now')
  })

  test('explains provider test failures without raw API text', async () => {
    settingsApiMock.testProvider.mockResolvedValue({
      ok: false,
      error: 'HTTP 403: Forbidden',
    })
    render(<ProvidersSection />)

    fireEvent.click(
      await screen.findByRole('button', { name: /check anthropic review connection/i })
    )

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(
        'Anthropic Review connection test failed. Check that the saved service access key is active and allowed to use the selected model, then save and check again.'
      )
    )
    expect(screen.queryByText(/HTTP 403/i)).toBeNull()
  })

  test('shows a beginner recovery step instead of raw provider setting details', async () => {
    useSettingsStore.setState({
      providers: [],
      providersError:
        'Check the required fields for provider, then try again. Code: 422. Details: API key is required',
    })

    render(<ProvidersSection />)

    await waitFor(() => expect(loadProvidersMock).toHaveBeenCalledTimes(1))
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Model service could not be saved. Choose the AI service, confirm the model, add the service access key, and add the service address if needed. Then save again.'
    )
    expect(screen.queryByText(/Details: API key is required/i)).toBeNull()
  })
})
