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

  test('guides an empty provider setup into the catalog and saves a built-in vendor', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    expect(within(nextStep).getByText('Add Your First Provider')).toBeDefined()
    expect(within(nextStep).getByText(/paste the key/i)).toBeDefined()

    fireEvent.click(within(nextStep).getByRole('button', { name: /add provider/i }))

    // The built-in catalog is the default Add view: a grid of vendor cards.
    const catalog = screen.getByRole('group', { name: /built-in provider catalog/i })
    expect(within(catalog).getByRole('button', { name: /anthropic/i })).toBeDefined()

    // Selecting a vendor opens the minimal inline config (model prefilled).
    fireEvent.click(within(catalog).getByRole('button', { name: /anthropic/i }))

    expect(screen.getByLabelText(/^model$/i)).toHaveValue('claude-sonnet-4-20250514')
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(/next: paste api key/i)
    const saveButton = screen.getByRole('button', { name: /save provider/i })

    // Save stays blocked until the key is present.
    expect(saveButton).toBeDisabled()
    expect(saveProviderMock).not.toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: 'sk-test' } })

    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(/ready to save/i)
    expect(saveButton).toBeEnabled()
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

  test('offers a Custom / Gateway path limited to bring-your-own endpoints', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    fireEvent.click(within(nextStep).getByRole('button', { name: /add provider/i }))
    fireEvent.click(screen.getByRole('button', { name: /custom \/ gateway/i }))

    // The full bring-your-own form is shown with a base URL field.
    expect(screen.getByText('Custom / Gateway setup path')).toBeDefined()
    const providerSelect = screen.getByLabelText(/^provider$/i)
    expect(within(providerSelect).getByRole('option', { name: 'OpenAI-Compatible' })).toBeDefined()
    expect(within(providerSelect).getByRole('option', { name: 'LiteLLM Gateway' })).toBeDefined()
    expect(within(providerSelect).getByRole('option', { name: 'OpenRouter' })).toBeDefined()
    // Curated vendors do NOT appear in the gateway dropdown.
    expect(within(providerSelect).queryByRole('option', { name: 'Anthropic' })).toBeNull()
    expect(within(providerSelect).queryByRole('option', { name: 'Zhipu GLM' })).toBeNull()
    expect(screen.getByLabelText(/base url/i)).toBeDefined()
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

    // The disabled provider still lists in the configured rows; the catalog
    // opens for adding an active one.
    expect(screen.getByText('Local Disabled')).toBeDefined()
    expect(screen.getByRole('group', { name: /built-in provider catalog/i })).toBeDefined()
  })

  test('collapses coding-plan variants into one vendor with Plan and Region toggles', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    fireEvent.click(within(nextStep).getByRole('button', { name: /add provider/i }))

    const catalog = screen.getByRole('group', { name: /built-in provider catalog/i })
    // One Zhipu card — not separate base + Coding Plan entries.
    expect(within(catalog).queryByRole('button', { name: /zhipu glm coding plan/i })).toBeNull()
    fireEvent.click(within(catalog).getByRole('button', { name: /zhipu glm/i }))

    // Model prefills from the API variant; Plan + Region toggles are present.
    expect(screen.getByLabelText(/^model$/i)).toHaveValue('glm-4.7')
    expect(screen.getByRole('group', { name: /^plan$/i })).toBeDefined()
    expect(screen.getByRole('group', { name: /^region$/i })).toBeDefined()

    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: 'sk-zhipu' } })

    // CN (default) → China base URL on the API plan.
    fireEvent.click(screen.getByRole('button', { name: /save provider/i }))
    await waitFor(() =>
      expect(saveProviderMock).toHaveBeenCalledWith(
        expect.objectContaining({
          provider: 'zhipu',
          model: 'glm-4.7',
          apiKey: 'sk-zhipu',
          baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
        })
      )
    )
  })

  test('Region=Global switches a vendor to its global endpoint on save', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    fireEvent.click(within(nextStep).getByRole('button', { name: /add provider/i }))

    const catalog = screen.getByRole('group', { name: /built-in provider catalog/i })
    fireEvent.click(within(catalog).getByRole('button', { name: /zhipu glm/i }))

    // Switch to the Coding Plan and Global region.
    fireEvent.click(
      within(screen.getByRole('group', { name: /^plan$/i })).getByRole('button', {
        name: /coding plan/i,
      })
    )
    fireEvent.click(
      within(screen.getByRole('group', { name: /^region$/i })).getByRole('button', {
        name: /global/i,
      })
    )
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: 'sk-zhipu' } })
    fireEvent.click(screen.getByRole('button', { name: /save provider/i }))

    await waitFor(() =>
      expect(saveProviderMock).toHaveBeenCalledWith(
        expect.objectContaining({
          provider: 'zhipu_coding',
          model: 'glm-4.7',
          apiKey: 'sk-zhipu',
          baseUrl: 'https://api.z.ai/api/anthropic',
        })
      )
    )
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
