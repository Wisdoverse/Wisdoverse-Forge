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
    expect(nextStep).toHaveTextContent('Work tool packages')
    expect(screen.getByText('Before assigning work')).toBeDefined()
    expect(screen.getByText('2/4 ready')).toBeDefined()
    expect(screen.getAllByText('Managed workspace').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Text-only model service').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Codex').length).toBeGreaterThan(0)
    expect(screen.queryByText('container')).toBeNull()
    expect(screen.queryByText('api')).toBeNull()
    expect(screen.queryByText('codex')).toBeNull()
    expect(screen.getAllByText(/Rebuild the agent tool packages/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Rebuild needed')).toBeDefined()
    expect(screen.getByText('Ready to use')).toBeDefined()
    expect(screen.getByRole('button', { name: /Connect GitHub/i })).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /Connect GitHub/i }))

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
    expect(screen.queryByRole('button', { name: /Connect GitHub/i })).toBeNull()
    expect(screen.getByText(/1\/1 work tools are ready/i)).toBeDefined()
  })

  test('shows beginner guidance when local tool sign-in status cannot load', async () => {
    agentApiMock.getCliAuthProxyStatus.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<RuntimeSection />)

    expect(
      await screen.findByText(/work tool account connection status could not load/i)
    ).toBeDefined()
    expect(screen.getByText(/app could not reach the service/i)).toBeDefined()
    expect(screen.queryByText(/failed to fetch/i)).toBeNull()
  })

  test('shows beginner guidance when agent online status cannot load', async () => {
    orchestrationApiMock.getParticipants.mockRejectedValueOnce(new Error('401 Unauthorized'))

    render(<RuntimeSection />)

    expect(await screen.findByText(/sign in again/i)).toBeDefined()
    expect(screen.queryByText(/code: 401/i)).toBeNull()
    expect(screen.queryByText(/Code:/i)).toBeNull()
    expect(screen.queryByText(/401 Unauthorized/)).toBeNull()
  })

  test('shows beginner guidance when local tool sign-in cannot start', async () => {
    agentApiMock.startCliAuthProxyLogin.mockResolvedValueOnce({
      ok: false,
      error: '403 Forbidden',
    })

    render(<RuntimeSection />)

    await screen.findByTestId('runtime-launch-checklist')
    fireEvent.click(screen.getByRole('button', { name: /Connect GitHub/i }))

    expect(await screen.findByText(/do not have permission to manage agent setup/i)).toBeDefined()
    expect(screen.getByText(/owner or admin/i)).toBeDefined()
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
      'Agent work settings could not be saved. Choose an available work location and local tool, then save again.'
    )
    expect(screen.queryByText(/Details: default CLI tool is not available/i)).toBeNull()
  })
})
