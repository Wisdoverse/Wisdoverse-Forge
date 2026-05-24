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
    expect(screen.getByRole('button', { name: /test openai production connection/i })).toBeDefined()
    expect(screen.getByText('Anthropic Review')).toBeDefined()
    expect(screen.getByText('Local Lab')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: 'Needs Test' }))

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
