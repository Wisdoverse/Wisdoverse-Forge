import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { ProvidersSection } from '@app/features/settings/ProvidersSection'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { LlmProviderConfig } from '@app/shared/api/legacy/settingsApi'

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
const setProviderEnabledMock = vi.fn()
const deleteProviderMock = vi.fn().mockResolvedValue(true)
const originalLoadProviders = useSettingsStore.getState().loadProviders
const originalSaveProvider = useSettingsStore.getState().saveProvider
const originalSetProviderEnabled = useSettingsStore.getState().setProviderEnabled
const originalDeleteProvider = useSettingsStore.getState().deleteProvider

beforeEach(() => {
  settingsApiMock.getSupportedProviders.mockResolvedValue([])
  settingsApiMock.testProvider.mockResolvedValue({ ok: true, latencyMs: 42 })
  loadProvidersMock.mockClear()
  saveProviderMock.mockClear()
  setProviderEnabledMock.mockReset()
  setProviderEnabledMock.mockImplementation(async (id: string, isEnabled: boolean) => {
    let updatedProvider: LlmProviderConfig | null = null
    useSettingsStore.setState((state) => ({
      providers: state.providers.map((provider) => {
        if (provider.id !== id) return provider
        const updated = {
          ...provider,
          isEnabled,
          lastTestStatus: isEnabled ? ('untested' as const) : provider.lastTestStatus,
          lastTestErrorMessage: isEnabled ? undefined : provider.lastTestErrorMessage,
        }
        updatedProvider = updated
        return updated
      }),
    }))
    return updatedProvider
  })
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
    setProviderEnabled: setProviderEnabledMock,
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
    setProviderEnabled: originalSetProviderEnabled,
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
    expect(screen.getByRole('heading', { name: 'AI services' })).toBeDefined()
    expect(screen.queryByText('AI Services')).toBeNull()
    expect(screen.getByRole('button', { name: /^add AI service$/i })).toBeDefined()
    expect(screen.queryByText('Add AI Service')).toBeNull()
    const nextStep = screen.getByTestId('provider-next-step')
    expect(within(nextStep).getByText('Do this next')).toBeDefined()
    expect(within(nextStep).getByText('Check the AI service connection')).toBeDefined()
    expect(within(nextStep).getByText(/What success looks like:/)).toBeDefined()
    expect(within(nextStep).queryByText('Do This Next')).toBeNull()
    expect(within(nextStep).queryByText(/Success:/)).toBeNull()
    expect(
      screen.getByRole('button', { name: /check openai production AI service connection/i })
    ).toBeDefined()
    expect(
      screen.getByRole('button', { name: /turn off openai production AI service/i })
    ).toBeDefined()
    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.getByText('Local Lab')).toBeDefined()
    expect(screen.getByRole('button', { name: /turn on local lab AI service/i })).toBeDefined()
    expect(screen.getAllByText('Needs check').length).toBeGreaterThan(0)
    expect(screen.queryByText('Failed')).toBeNull()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Check the service access key, model, and service address for Anthropic Review'
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

  test('guides an empty provider setup into the catalog and saves a built-in vendor', async () => {
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
    expect(screen.getByText(/Use the step above to add one AI account/i)).toBeDefined()
    expect(screen.queryByText(/Use Do This Next above/i)).toBeNull()
    expect(screen.queryByText('No AI services configured')).toBeNull()

    fireEvent.click(within(nextStep).getByRole('button', { name: /add AI service/i }))

    expect(screen.getByRole('button', { name: /known AI services/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /custom service address/i })).toBeDefined()
    expect(screen.queryByText(/built-in catalog/i)).toBeNull()
    expect(screen.queryByText(/custom \/ gateway/i)).toBeNull()

    const serviceChoices = screen.getByRole('group', { name: /known AI services/i })
    expect(within(serviceChoices).getByRole('button', { name: /anthropic/i })).toBeDefined()
    fireEvent.click(within(serviceChoices).getByRole('button', { name: /anthropic/i }))

    expect(screen.getByLabelText(/^model to use$/i)).toHaveValue('claude-sonnet-4-20250514')
    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(
      /next: paste the service access key/i
    )
    expect(screen.getByText(/service website address and model/i)).toBeDefined()
    expect(screen.getByText(/paste the service access key and save/i)).toBeDefined()
    expect(screen.getByText(/some services call this an API key/i)).toBeDefined()
    expect(screen.getByText(/do not paste the sign-in password/i)).toBeDefined()
    expect(screen.getByText(/After saving, choose Check connection/i)).toBeDefined()
    expect(screen.getByText(/Ready means simple chat agents can use this service/i)).toBeDefined()
    expect(screen.queryByText(/Service address and model are filled in/i)).toBeNull()
    expect(screen.queryByText(/After saving, click Check/i)).toBeNull()
    const saveButton = screen.getByRole('button', { name: /save AI service/i })
    expect(saveButton).toBeDisabled()
    expect(saveProviderMock).not.toHaveBeenCalled()
    const accessKeyInput = screen.getByLabelText(/service access key/i)
    expect(accessKeyInput.getAttribute('aria-describedby')).toContain('provider-form-api-key-help')

    fireEvent.change(accessKeyInput, {
      target: { value: 'sk-test' },
    })

    expect(screen.getByTestId('provider-form-status')).toHaveTextContent(
      /ready to save this service/i
    )
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

  test('announces AI service setup errors as recovery guidance', async () => {
    useSettingsStore.setState({
      providers: [],
      providersError: 'Paste the service access key from the selected AI service, then save again.',
    })

    render(<ProvidersSection />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).not.toHaveTextContent('AI service could not be saved.')
    expect(alert).toHaveTextContent('save again')
  })

  test('offers a Custom / Gateway path limited to bring-your-own endpoints', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    fireEvent.click(within(nextStep).getByRole('button', { name: /add AI service/i }))
    fireEvent.click(screen.getByRole('button', { name: /custom service address/i }))

    expect(screen.getByText('3 steps to connect an AI account')).toBeDefined()
    expect(screen.getByText('Paste the service access key')).toBeDefined()
    expect(screen.getAllByText(/service access key from that AI account/i).length).toBeGreaterThan(
      0
    )
    expect(screen.getAllByText(/do not paste the sign-in password/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Save, then check connection')).toBeDefined()
    expect(screen.queryByText('Paste service access key')).toBeNull()
    expect(screen.queryByText('Save and check')).toBeNull()
    expect(screen.queryByText(/copy its access key/i)).toBeNull()
    const providerSelect = screen.getByLabelText(/^AI service$/i)
    expect(within(providerSelect).getByRole('option', { name: 'OpenAI-Compatible' })).toBeDefined()
    expect(within(providerSelect).getByRole('option', { name: 'LiteLLM Gateway' })).toBeDefined()
    expect(within(providerSelect).getByRole('option', { name: 'OpenRouter' })).toBeDefined()
    expect(within(providerSelect).queryByRole('option', { name: 'Anthropic' })).toBeNull()
    expect(within(providerSelect).queryByRole('option', { name: 'Zhipu GLM' })).toBeNull()
    expect(screen.getByLabelText(/service address/i)).toBeDefined()
    expect(screen.queryByLabelText(/^private key/i)).toBeNull()
    expect(screen.queryByText(/gateway setup path/i)).toBeNull()
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
    expect(within(readiness).getByText('Finish AI service setup')).toBeDefined()
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
    expect(within(nextStep).getByText('Turn on or replace an AI service')).toBeDefined()
    expect(within(nextStep).getByText(/Show the disabled list/i)).toBeDefined()

    fireEvent.click(within(nextStep).getByRole('button', { name: /show disabled services/i }))

    // The disabled provider still lists in the configured rows.
    expect(screen.getByText('Local Disabled')).toBeDefined()
    expect(screen.getByRole('button', { name: 'Disabled' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.queryByRole('button', { name: /save AI service/i })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /turn on local disabled AI service/i }))

    await waitFor(() =>
      expect(setProviderEnabledMock).toHaveBeenCalledWith('provider-disabled-only', true)
    )
    expect(screen.getByRole('button', { name: 'Needs check' })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    expect(
      screen.getByRole('button', { name: /check local disabled AI service connection/i })
    ).toBeEnabled()
    expect(screen.queryByRole('button', { name: /save AI service/i })).toBeNull()
  })

  test('points ready AI service setup toward Create Agent', async () => {
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

    const nextStep = await screen.findByTestId('provider-next-step')
    expect(within(nextStep).getByText('Ready to create simple chat agents')).toBeDefined()
    expect(within(nextStep).getByText(/choose Create Agent/i)).toBeDefined()
    expect(within(nextStep).queryByText(/choose New Agent/i)).toBeNull()
  })

  test('collapses coding-plan variants into one vendor with beginner-friendly setup choices', async () => {
    useSettingsStore.setState({ providers: [] })

    render(<ProvidersSection />)

    const nextStep = await screen.findByTestId('provider-next-step')
    fireEvent.click(within(nextStep).getByRole('button', { name: /add AI service/i }))

    const serviceChoices = screen.getByRole('group', { name: /known AI services/i })
    expect(within(serviceChoices).queryByRole('button', { name: /zhipu glm coding plan/i })).toBeNull()
    const zhipuChoice = within(serviceChoices).getByRole('button', { name: /zhipu glm/i })
    expect(
      within(zhipuChoice).getByText(/standard setup · coding plan · china or global website address/i)
    ).toBeDefined()
    expect(within(zhipuChoice).queryByText(/api · coding plan/i)).toBeNull()
    expect(within(zhipuChoice).queryByText(/cn\/global/i)).toBeNull()
    expect(within(zhipuChoice).queryByText(/china\/global address/i)).toBeNull()
    fireEvent.click(zhipuChoice)

    expect(screen.getByLabelText(/^model to use$/i)).toHaveValue('glm-4.7')
    expect(screen.getByRole('group', { name: /^service plan$/i })).toBeDefined()
    expect(screen.getByRole('group', { name: /^service website region$/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /^standard$/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /^coding plan$/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /^china$/i })).toBeDefined()
    expect(screen.queryByRole('group', { name: /^plan$/i })).toBeNull()
    expect(screen.queryByRole('group', { name: /^region$/i })).toBeNull()
    expect(screen.queryByRole('group', { name: /^service address region$/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /^cn$/i })).toBeNull()

    fireEvent.change(screen.getByLabelText(/service access key/i), {
      target: { value: 'sk-zhipu' },
    })

    fireEvent.click(screen.getByRole('button', { name: /save AI service/i }))
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
    fireEvent.click(within(nextStep).getByRole('button', { name: /add AI service/i }))

    const serviceChoices = screen.getByRole('group', { name: /known AI services/i })
    fireEvent.click(within(serviceChoices).getByRole('button', { name: /zhipu glm/i }))

    // Switch to the coding plan and global service address.
    fireEvent.click(
      within(screen.getByRole('group', { name: /^service plan$/i })).getByRole('button', {
        name: /coding plan/i,
      })
    )
    fireEvent.click(
      within(screen.getByRole('group', { name: /^service website region$/i })).getByRole('button', {
        name: /global/i,
      })
    )
    fireEvent.change(screen.getByLabelText(/service access key/i), {
      target: { value: 'sk-zhipu' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save AI service/i }))

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
      await screen.findByRole('button', { name: /check anthropic review AI service connection/i })
    )

    await waitFor(() =>
      expect(settingsApiMock.testProvider).toHaveBeenCalledWith('provider-needs-test')
    )
    expect(loadProvidersMock).toHaveBeenCalled()
  })

  test('uses clear remove labels before deleting an AI service', async () => {
    let finishDelete: (removed: boolean) => void = () => {}
    deleteProviderMock.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          finishDelete = resolve
        })
    )

    render(<ProvidersSection />)

    const removeButton = await screen.findByRole('button', {
      name: /remove anthropic review AI service/i,
    })
    expect(removeButton).toHaveTextContent('Remove AI service')

    fireEvent.click(removeButton)

    expect(deleteProviderMock).not.toHaveBeenCalled()
    expect(screen.getByRole('note')).toHaveTextContent(
      'Removing this service stops agents from using Anthropic Review.'
    )
    expect(screen.getByRole('button', { name: /keep anthropic review AI service/i })).toHaveTextContent(
      'Keep service'
    )
    expect(
      screen.getByRole('button', { name: /remove anthropic review AI service now/i })
    ).toHaveTextContent('Remove now')
    expect(screen.queryByRole('button', { name: /^delete$/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /^confirm\\?$/i })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /keep anthropic review AI service/i }))
    expect(deleteProviderMock).not.toHaveBeenCalled()
    expect(screen.queryByRole('note')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /remove anthropic review AI service/i }))
    const confirmButton = screen.getByRole('button', {
      name: /remove anthropic review AI service now/i,
    })
    fireEvent.click(confirmButton)
    expect(confirmButton).toHaveTextContent('Removing...')
    expect(confirmButton).toBeDisabled()

    await act(async () => {
      finishDelete(true)
    })

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

    const alert = await screen.findByText(/Try checking Anthropic Review again in a few minutes/i)
    expect(alert).toHaveTextContent('ask an owner or admin to check AI service settings')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('provider gateway stack trace')
  })
})
