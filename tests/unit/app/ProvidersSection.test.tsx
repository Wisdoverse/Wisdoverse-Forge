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
    expect(within(readiness).getByText(/1 AI service is ready to use/i)).toBeDefined()
    expect(within(readiness).getByText(/1 AI service needs a connection check/i)).toBeDefined()
    expect(within(readiness).getByText(/1 AI service is disabled/i)).toBeDefined()
    expect(within(readiness).queryByText(/still needs Check/i)).toBeNull()
    expect(within(readiness).queryByText(new RegExp('1/3 AI services\\s+ready', 'i'))).toBeNull()
    expect(within(readiness).getByText('Default: OpenAI Production')).toBeDefined()
    expect(within(readiness).getByText('Default AI service')).toBeDefined()
    expect(within(readiness).queryByText('Default Route')).toBeNull()
    const nextStep = screen.getByTestId('provider-next-step')
    expect(within(nextStep).getByText('Do This Next')).toBeDefined()
    expect(within(nextStep).getByText('Check the AI service connection')).toBeDefined()
    expect(
      screen.getByRole('button', { name: /check openai production AI service connection/i })
    ).toBeDefined()
    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.getByText('Local Lab')).toBeDefined()
    expect(screen.getAllByText('Needs check').length).toBeGreaterThan(0)
    expect(screen.queryByText('Failed')).toBeNull()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Anthropic Review connection check needs attention.'
    )
    expect(screen.getByRole('alert')).toHaveTextContent('service access key')
    expect(screen.queryByText('Invalid key')).toBeNull()

    fireEvent.click(within(nextStep).getByRole('button', { name: /show needs check/i }))

    expect(
      screen.queryByRole('button', { name: /check openai production AI service connection/i })
    ).toBeNull()
    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(
      screen.queryByRole('button', { name: /check local lab AI service connection/i })
    ).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Disabled' }))

    expect(screen.queryByText('Anthropic Review')).toBeNull()
    expect(screen.getByText('Local Lab')).toBeDefined()
  })

  test('searches providers and exposes a clear empty state', async () => {
    render(<ProvidersSection />)

    fireEvent.change(await screen.findByRole('searchbox', { name: /search AI services/i }), {
      target: { value: 'review' },
    })

    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.queryByRole('button', { name: /check openai production connection/i })).toBeNull()

    fireEvent.change(screen.getByRole('searchbox', { name: /search AI services/i }), {
      target: { value: 'missing-provider' },
    })

    const searchEmpty = screen.getByTestId('provider-filter-empty')
    expect(within(searchEmpty).getByText('Clear search to see AI services')).toBeDefined()
    expect(searchEmpty.textContent).toContain(
      'Your AI services exist, but this search hides them. Try a broader name.'
    )
    expect(searchEmpty.textContent).not.toContain('No AI services match this view')

    fireEvent.click(screen.getByRole('button', { name: /show all AI services/i }))
    expect(screen.getAllByText('OpenAI Production').length).toBeGreaterThan(0)
    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.getByText('Local Lab')).toBeDefined()
  })

  test('explains filter-only and combined empty AI service views', async () => {
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-ready-only',
          provider: 'openai',
          displayName: 'OpenAI Production',
          model: 'gpt-5.4',
          priority: 1,
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        },
      ],
    })

    render(<ProvidersSection />)

    expect((await screen.findAllByText('OpenAI Production')).length).toBeGreaterThan(0)
    fireEvent.click(screen.getByRole('button', { name: 'Needs check' }))

    const filterEmpty = screen.getByTestId('provider-filter-empty')
    expect(within(filterEmpty).getByText('Choose All to see AI services')).toBeDefined()
    expect(filterEmpty.textContent).toContain(
      'Your AI services exist, but this filter has no results yet.'
    )
    expect(filterEmpty.textContent).not.toContain('No AI services match this view')

    fireEvent.change(screen.getByRole('searchbox', { name: /search AI services/i }), {
      target: { value: 'openai' },
    })

    const combinedEmpty = screen.getByTestId('provider-filter-empty')
    expect(within(combinedEmpty).getByText('Clear search or show all AI services')).toBeDefined()
    expect(combinedEmpty.textContent).toContain(
      'Your AI services exist, but the current search and filter hide them.'
    )
    expect(combinedEmpty.textContent).not.toContain('No AI services match this view')
  })

  test('guides an empty provider setup into the add form', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    const readiness = screen.getByTestId('provider-readiness')
    expect(within(readiness).getByText('Default: add an AI service first')).toBeDefined()
    expect(within(readiness).getByText('Add first service')).toBeDefined()
    expect(within(readiness).queryByText('Default: None')).toBeNull()
    expect(within(readiness).queryByText('Not set')).toBeNull()
    expect(within(nextStep).getByText('Add your first AI service')).toBeDefined()
    expect(within(nextStep).getByText(/paste the service access key/i)).toBeDefined()
    expect(screen.getAllByText('Add your first AI service').length).toBeGreaterThan(1)
    expect(screen.getByText(/Use Do This Next above to add one AI account/i)).toBeDefined()
    expect(screen.queryByText('No AI services configured')).toBeNull()

    fireEvent.click(within(nextStep).getByRole('button', { name: /add AI service/i }))

    expect(screen.getByText('3 steps to connect an AI account')).toBeDefined()
    expect(screen.getByText('Paste service access key')).toBeDefined()
    expect(screen.getAllByText(/copy its access key/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Save and check')).toBeDefined()
    expect(screen.getAllByText(/Ready means agents can use/i).length).toBeGreaterThan(0)
    expect(screen.getByLabelText(/^AI service$/i)).toBeDefined()
    expect(screen.getByLabelText(/^Name in Forge$/i)).toBeDefined()
    expect(screen.getByText(/suggested model is safe to start with/i)).toBeDefined()
    expect(screen.queryByText(/paste private key/i)).toBeNull()
    expect(screen.queryByLabelText(/^private key/i)).toBeNull()
    expect(screen.queryByLabelText(/^Display Name$/i)).toBeNull()
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(
      /next: paste the service access key/i
    )
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(
      /some services call this an API key/i
    )
    expect(screen.getByText(/Open your AI service account, copy its access key/i)).toBeDefined()
    const saveButton = screen.getByRole('button', { name: /save AI service/i })
    expect(saveButton).toBeEnabled()

    fireEvent.click(saveButton)

    expect(
      screen.getAllByText('Paste the service access key before saving this AI service.').length
    ).toBeGreaterThan(0)
    expect(saveProviderMock).not.toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText(/service access key/i), {
      target: { value: 'sk-test' },
    })

    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(
      /ready to save this service/i
    )
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(
      /Ready means agents can use it/i
    )
    expect(screen.getByTestId('provider-form-status').textContent).not.toMatch(
      new RegExp(['run', 'Check'].join('\\s+'), 'i')
    )
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

  test('announces AI service setup errors as recovery guidance', async () => {
    useSettingsStore.setState({
      providers: [],
      providersError:
        'AI service could not be saved. Paste the service access key from the selected AI service, then save again.',
    })

    render(<ProvidersSection />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('AI service could not be saved.')
    expect(alert).toHaveTextContent('save again')
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
    expect(within(readiness).getByText('AI service setup needs attention')).toBeDefined()
    expect(
      within(readiness).getByText(/Enable or add an AI service before agents can use one/i)
    ).toBeDefined()
    expect(within(readiness).getByText(/no connection checks are needed/i)).toBeDefined()
    expect(within(readiness).getByText(/1 AI service is disabled/i)).toBeDefined()
    expect(within(readiness).queryByText(/No AI services are ready to use yet/i)).toBeNull()
    expect(within(readiness).queryByText(/none need Check/i)).toBeNull()
    expect(within(readiness).getByText('Default: choose a ready AI service')).toBeDefined()
    expect(within(readiness).getByText('Choose a default')).toBeDefined()
    expect(within(readiness).queryByText('Default: None')).toBeNull()
    expect(within(readiness).queryByText('Not set')).toBeNull()
    const nextStep = screen.getByTestId('provider-next-step')
    expect(within(nextStep).getByText('Add a working AI service')).toBeDefined()

    fireEvent.click(within(nextStep).getByRole('button', { name: /add AI service/i }))

    expect(screen.getByText('Local Disabled')).toBeDefined()
    expect(screen.getByRole('button', { name: /save AI service/i })).toBeEnabled()
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(
      /next: paste the service access key/i
    )
  })

  test('surfaces the China default placeholder and global address hint for region-switch providers', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    fireEvent.click(within(nextStep).getByRole('button', { name: /add AI service/i }))

    fireEvent.change(screen.getByLabelText(/^AI service$/i), { target: { value: 'zhipu' } })

    // China address is the default (placeholder); the global address is the hint.
    expect(screen.getByLabelText(/^model to use$/i)).toHaveValue('glm-4.7')
    expect(screen.getByLabelText(/service address/i)).toHaveAttribute(
      'placeholder',
      expect.stringContaining('https://open.bigmodel.cn/api/paas/v4')
    )
    expect(
      screen.getByText(/global address, paste this: https:\/\/api\.z\.ai\/api\/paas\/v4/i)
    ).toBeDefined()

    // Hunyuan is China-only: no global address hint, default copy returns.
    fireEvent.change(screen.getByLabelText(/^AI service$/i), { target: { value: 'hunyuan' } })
    expect(screen.getByText(/Most users leave this blank/i)).toBeDefined()
    expect(screen.queryByText(/global address, paste this:/i)).toBeNull()
  })

  test('runs a provider test from the provider row', async () => {
    render(<ProvidersSection />)

    fireEvent.click(
      await screen.findByRole('button', { name: /check anthropic review AI service connection/i })
    )

    await waitFor(() =>
      expect(settingsApiMock.testProvider).toHaveBeenCalledWith('provider-needs-test')
    )
    expect(loadProvidersMock).toHaveBeenCalled()
  })

  test('uses clear remove labels before deleting an AI service', async () => {
    render(<ProvidersSection />)

    const removeButton = await screen.findByRole('button', {
      name: /remove anthropic review AI service/i,
    })
    expect(removeButton).toHaveTextContent('Remove AI service')

    fireEvent.click(removeButton)

    expect(deleteProviderMock).not.toHaveBeenCalled()
    const confirmButton = screen.getByRole('button', {
      name: /confirm removing anthropic review AI service/i,
    })
    expect(confirmButton).toHaveTextContent('Confirm remove')
    expect(screen.queryByRole('button', { name: /^delete$/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /^confirm\\?$/i })).toBeNull()

    fireEvent.click(confirmButton)

    await waitFor(() => {
      expect(deleteProviderMock).toHaveBeenCalledWith('provider-needs-test')
    })
  })

  test('hides raw provider check failures from the provider row', async () => {
    settingsApiMock.testProvider.mockResolvedValueOnce({
      ok: false,
      error: 'HTTP 500: provider gateway stack trace',
    })

    render(<ProvidersSection />)

    fireEvent.click(
      await screen.findByRole('button', { name: /check anthropic review AI service connection/i })
    )

    const alert = await screen.findByText(/Forge could not check this AI service right now/i)
    expect(alert).toHaveTextContent('ask an owner or admin to check AI service settings')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('provider gateway stack trace')
  })
})
