import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import '@app/i18n'
import { RuntimeSection } from '@app/features/settings/RuntimeSection'
import { useSettingsStore } from '@app/shared/model/settings.store'

const { agentApiMock, orchestrationApiMock } = vi.hoisted(() => ({
  agentApiMock: {
    getCliAuthProxyStatus: vi.fn(),
    startCliAuthProxyLogin: vi.fn(),
  },
  orchestrationApiMock: {
    getParticipants: vi.fn(),
  },
}))

vi.mock('@app/shared/api/legacy', () => ({
  getAgentApi: () => agentApiMock,
  getSettingsApi: () => ({
    getRuntimeSettings: vi.fn(),
    updateRuntimeSettings: vi.fn(),
  }),
}))

vi.mock('@app/shared/api/orchestration', () => ({
  orchestrationApi: orchestrationApiMock,
}))

const loadRuntimeSettingsMock = vi.fn().mockResolvedValue(undefined)
const updateRuntimeSettingsMock = vi.fn().mockResolvedValue(true)
const originalLoadRuntimeSettings = useSettingsStore.getState().loadRuntimeSettings
const originalUpdateRuntimeSettings = useSettingsStore.getState().updateRuntimeSettings

beforeEach(() => {
  agentApiMock.getCliAuthProxyStatus.mockResolvedValue({
    ok: true,
    statuses: [
      {
        provider: 'github',
        displayName: 'GitHub',
        cliTool: 'codex',
        connected: false,
        revokeReason: 'token expired',
      },
      {
        provider: 'gitlab',
        displayName: 'GitLab',
        cliTool: 'claude',
        connected: true,
        lastRefresh: '2026-05-20T12:00:00.000Z',
      },
    ],
  })
  agentApiMock.startCliAuthProxyLogin.mockResolvedValue({
    ok: true,
    url: 'https://auth.example.test/start',
  })
  orchestrationApiMock.getParticipants.mockResolvedValue([
    {
      id: 'participant-1',
      agentId: 'agent-1',
      name: 'Builder Agent',
      status: 'available',
      capabilities: [],
      lastHeartbeatAt: '2026-05-20T12:01:00.000Z',
    },
  ])
  loadRuntimeSettingsMock.mockClear()
  updateRuntimeSettingsMock.mockClear()
  useSettingsStore.setState({
    runtimeSettings: {
      defaultRuntime: 'container',
      availableRuntimes: ['container', 'api'],
      defaultCliTool: 'codex',
      availableCliTools: ['codex', 'claude'],
      cliToolDetails: [
        {
          cliTool: 'codex',
          image: 'agentforge-agent:codex',
          imagePresent: false,
          versionSource: 'not-reported',
        },
        {
          cliTool: 'claude',
          image: 'agentforge-agent:claude',
          imagePresent: true,
          version: '1.0.0',
          versionSource: 'docker-label',
        },
      ],
    },
    runtimeLoading: false,
    runtimeError: null,
    loadRuntimeSettings: loadRuntimeSettingsMock,
    updateRuntimeSettings: updateRuntimeSettingsMock,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useSettingsStore.setState({
    runtimeSettings: null,
    runtimeLoading: false,
    runtimeError: null,
    loadRuntimeSettings: originalLoadRuntimeSettings,
    updateRuntimeSettings: originalUpdateRuntimeSettings,
  })
})

describe('RuntimeSection', () => {
  test('shows work setup actions for missing tools and disconnected sign-ins', async () => {
    const openSpy = vi.spyOn(window, 'open').mockImplementation(() => null)

    render(<RuntimeSection />)

    expect(await screen.findByTestId('runtime-launch-checklist')).toBeDefined()
    const nextStep = screen.getByTestId('runtime-next-step')
    expect(nextStep).toHaveTextContent('Do this next')
    expect(nextStep).toHaveTextContent('Work tools')
    expect(screen.getByText('Before assigning work')).toBeDefined()
    expect(screen.getByText('2/4 ready')).toBeDefined()
    expect(screen.getByText(/work locations available/i)).toBeDefined()
    expect(screen.queryByText(/work places available/i)).toBeNull()
    expect(screen.getByText(/work tools available/i)).toBeDefined()
    expect(screen.getAllByText(/project files or live work access/i).length).toBeGreaterThan(0)
    expect(screen.getAllByText('Managed workspace').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Chat-only AI service').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Codex').length).toBeGreaterThan(0)
    expect(screen.queryByText(/command window/i)).toBeNull()
    expect(screen.queryByText(/Text-only model service/i)).toBeNull()
    expect(screen.queryByText('container')).toBeNull()
    expect(screen.queryByText('api')).toBeNull()
    expect(screen.queryByText('codex')).toBeNull()
    expect(screen.getAllByText(/Rebuild work tools/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/agent tools/i)).toBeNull()
    expect(screen.queryByText(/agent check-in/i)).toBeNull()
    expect(screen.queryByText(/account sign-ins/i)).toBeNull()
    expect(screen.queryByText(/tool check|not reported|not checked/i)).toBeNull()
    expect(screen.queryByText(/package check/i)).toBeNull()
    expect(screen.getByText('Rebuild needed')).toBeDefined()
    expect(screen.getByText('Ready to use')).toBeDefined()
    expect(screen.getAllByText(/tool account connections/i).length).toBeGreaterThan(0)
    expect(screen.getByText(/1\/2 tool account connections ready/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /Reconnect GitHub account/i })).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /Reconnect GitHub account/i }))

    await waitFor(() => expect(agentApiMock.startCliAuthProxyLogin).toHaveBeenCalledWith('github'))
    expect(openSpy).toHaveBeenCalledWith(
      'https://auth.example.test/start',
      '_blank',
      'noopener,noreferrer'
    )
  })

  test('summarizes the ready work setup without connection actions', async () => {
    agentApiMock.getCliAuthProxyStatus.mockResolvedValueOnce({
      ok: true,
      statuses: [
        {
          provider: 'github',
          displayName: 'GitHub',
          cliTool: 'codex',
          connected: true,
          lastRefresh: '2026-05-20T12:00:00.000Z',
        },
      ],
    })
    useSettingsStore.setState({
      runtimeSettings: {
        defaultRuntime: 'container',
        availableRuntimes: ['container'],
        defaultCliTool: 'codex',
        availableCliTools: ['codex'],
        cliToolDetails: [
          {
            cliTool: 'codex',
            image: 'agentforge-agent:codex',
            imagePresent: true,
            version: '1.0.0',
            versionSource: 'docker-label',
          },
        ],
      },
    })

    render(<RuntimeSection />)

    expect(await screen.findByText('4/4 ready')).toBeDefined()
    expect(screen.getByTestId('runtime-next-step')).toHaveTextContent('Ready to start agent work')
    expect(screen.getByTestId('runtime-next-step')).toHaveTextContent('The work location')
    expect(screen.queryByRole('button', { name: /Reconnect GitHub account/i })).toBeNull()
    expect(screen.getByText(/1\/1 work tools are ready/i)).toBeDefined()
    expect(screen.getByText(/1\/1 tool account connections ready/i)).toBeDefined()
    expect(screen.getAllByText(/agent online status/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/agent tools are checked/i)).toBeNull()
    expect(screen.queryByText(/agent check-ins/i)).toBeNull()
  })

  test('labels missing work setup clearly instead of Unknown', async () => {
    useSettingsStore.setState({
      runtimeSettings: null,
      runtimeLoading: false,
      runtimeError: null,
    })

    render(<RuntimeSection />)

    expect(await screen.findByText('Agent Work Setup has not loaded yet.')).toBeDefined()
    expect(screen.getByText('Not set yet')).toBeDefined()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('labels unknown work location and tool values without exposing backend codes', async () => {
    useSettingsStore.setState({
      runtimeSettings: {
        defaultRuntime: 'future_runtime' as never,
        availableRuntimes: ['future_runtime' as never],
        defaultCliTool: 'future_tool' as never,
        availableCliTools: ['future_tool' as never],
        cliToolDetails: [
          {
            cliTool: 'future_tool' as never,
            image: 'agentforge-agent:future-tool',
            imagePresent: true,
            version: '1.0.0',
            versionSource: 'docker-label',
          },
        ],
      },
    })

    render(<RuntimeSection />)

    expect((await screen.findAllByText('Work location needs review')).length).toBeGreaterThan(0)
    expect(screen.getAllByText('Work tool needs review').length).toBeGreaterThan(0)
    expect(screen.queryByText(/future_runtime/i)).toBeNull()
    expect(screen.queryByText(/future_tool/i)).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('shows beginner guidance when local tool sign-in status cannot load', async () => {
    agentApiMock.getCliAuthProxyStatus.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<RuntimeSection />)

    expect(
      await screen.findByText(/tool account connection could not be checked/i)
    ).toBeDefined()
    expect(screen.getByText(/Forge could not connect while checking Agent Work Setup/i)).toBeDefined()
    expect(screen.queryByText(/failed to fetch/i)).toBeNull()
    expect(screen.queryByText(/app could not reach/i)).toBeNull()
    expect(screen.queryByText(/service is healthy/i)).toBeNull()
  })

  test('shows beginner guidance when agent online status cannot load', async () => {
    orchestrationApiMock.getParticipants.mockRejectedValueOnce(new Error('401 Unauthorized'))

    render(<RuntimeSection />)

    expect(await screen.findByText(/sign in again/i)).toBeDefined()
    expect(screen.queryByText(/code: 401/i)).toBeNull()
    expect(screen.queryByText(/Code:/i)).toBeNull()
    expect(screen.queryByText(/401 Unauthorized/)).toBeNull()
    expect(screen.queryByText(/service is healthy/i)).toBeNull()
  })

  test('shows beginner guidance when local tool sign-in cannot start', async () => {
    agentApiMock.startCliAuthProxyLogin.mockResolvedValueOnce({
      ok: false,
      error: '403 Forbidden',
    })

    render(<RuntimeSection />)

    await screen.findByTestId('runtime-launch-checklist')
    fireEvent.click(screen.getByRole('button', { name: /Reconnect GitHub account/i }))

    expect(await screen.findByText(/do not have permission to manage Agent Work Setup/i)).toBeDefined()
    expect(screen.getAllByText(/owner or admin/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/403 Forbidden/)).toBeNull()
  })

  test('shows beginner guidance instead of raw runtime setting details', async () => {
    useSettingsStore.setState({
      runtimeError:
        'Check the required fields for runtime setting, then try again. Code: 422. Details: default CLI tool is not available',
    })

    render(<RuntimeSection />)

    await screen.findByTestId('runtime-launch-checklist')
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Agent Work Setup could not be saved. Choose an available work location and local tool, then save again.'
    )
    expect(screen.queryByText(/Details: default CLI tool is not available/i)).toBeNull()
  })
})
