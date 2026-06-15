import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
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
    const readiness = screen.getByTestId('runtime-readiness')
    const nextStep = screen.getByTestId('runtime-next-step')
    expect(nextStep).toHaveTextContent('Next step')
    expect(nextStep).toHaveTextContent('Work tools ready')
    expect(nextStep).toHaveTextContent('What success looks like: This item changes to Ready.')
    expect(nextStep).not.toHaveTextContent('Success:')
    expect(screen.getByText('Before assigning work')).toBeDefined()
    expect(screen.getByText('2/4 ready')).toBeDefined()
    expect(within(readiness).getByText('Finish agent work setup')).toBeDefined()
    expect(
      screen.getByText(/Setup has 2 agent locations and 2 work tools like Claude or Codex/i)
    ).toBeDefined()
    expect(
      screen.getByText(/1 work tool sign-in is connected, and 1 agent is online/i)
    ).toBeDefined()
    expect(
      within(readiness).queryByText(new RegExp('agent locations\\s+available', 'i'))
    ).toBeNull()
    expect(screen.queryByText(/work places available/i)).toBeNull()
    expect(
      screen.queryByText(new RegExp('tools like Claude or Codex\\s+available', 'i'))
    ).toBeNull()
    expect(screen.queryByText(/seen online/i)).toBeNull()
    expect(
      screen.getAllByText(/project files, commands, or live work access/i).length
    ).toBeGreaterThan(0)
    expect(
      screen.getByText(
        'Managed workspace is the simplest choice. Choose This computer only when this machine should join the workspace as a managed agent.'
      )
    ).toBeDefined()
    expect(screen.queryByText(new RegExp(['unless', 'owner', 'tells'].join('.*'), 'i'))).toBeNull()
    expect(screen.getAllByText(/work tool setup/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/tool install status/i)).toBeNull()
    expect(screen.getAllByText('Managed workspace').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Chat-only AI service').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Codex').length).toBeGreaterThan(0)
    expect(screen.queryByText(/command window/i)).toBeNull()
    expect(screen.queryByText(/Text-only model service/i)).toBeNull()
    expect(screen.queryByText('container')).toBeNull()
    expect(screen.queryByText('api')).toBeNull()
    expect(screen.queryByText('codex')).toBeNull()
    expect(
      screen.getAllByText(/ask an owner to finish setting up the tools/i).length
    ).toBeGreaterThan(0)
    expect(screen.queryByText(/agent tools/i)).toBeNull()
    expect(screen.queryByText(/agent check-in/i)).toBeNull()
    expect(screen.queryByText(/tool check|not reported|not checked/i)).toBeNull()
    expect(screen.queryByText(/package check/i)).toBeNull()
    expect(screen.getByText('Setup needed')).toBeDefined()
    expect(screen.getByText('Installed and ready')).toBeDefined()
    expect(screen.getAllByText(/work tool sign-ins/i).length).toBeGreaterThan(0)
    expect(screen.getByText(/1\/2 work tool sign-ins ready/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /Sign in to GitHub/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /Check setup/i })).toBeDefined()
    expect(
      screen.queryByRole('button', { name: new RegExp(['Check', 'status'].join(' '), 'i') })
    ).toBeNull()
    expect(screen.queryByRole('button', { name: /^Refresh$/i })).toBeNull()
    expect(screen.queryByText('Needs action')).toBeNull()
    expect(screen.getAllByText('Needs setup').length).toBeGreaterThan(0)

    fireEvent.click(screen.getByRole('button', { name: /Sign in to GitHub/i }))

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
    expect(
      screen.getByText(/Setup has 1 agent location and 1 work tool like Claude or Codex/i)
    ).toBeDefined()
    expect(
      screen.getByText(/1 work tool sign-in is connected, and 1 agent is online/i)
    ).toBeDefined()
    expect(screen.getByTestId('runtime-next-step')).toHaveTextContent('Ready to give agents work')
    expect(screen.getByTestId('runtime-next-step')).toHaveTextContent('The agent location')
    expect(screen.getByTestId('runtime-next-step')).toHaveTextContent(
      'What success looks like: Open Agents, create or select an agent, then assign work from Tasks.'
    )
    expect(screen.getByTestId('runtime-next-step')).not.toHaveTextContent('Success:')
    expect(screen.queryByRole('button', { name: /Sign in to GitHub/i })).toBeNull()
    expect(screen.getByText(/1\/1 work tools are ready/i)).toBeDefined()
    expect(screen.getByText(/1\/1 work tool sign-ins ready/i)).toBeDefined()
    expect(screen.getAllByText(/agent online status/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/agent tools are checked/i)).toBeNull()
    expect(screen.queryByText(/agent check-ins/i)).toBeNull()
  })

  test('tells users to sign in before starting agents when no work tool sign-ins are connected', async () => {
    agentApiMock.getCliAuthProxyStatus.mockResolvedValueOnce({
      ok: true,
      statuses: [
        {
          provider: 'github',
          displayName: 'GitHub',
          cliTool: 'codex',
          connected: false,
        },
      ],
    })

    render(<RuntimeSection />)

    expect(await screen.findByTestId('runtime-launch-checklist')).toBeDefined()
    expect(
      screen.getByText(/Sign in to a work tool before starting agents that need one/i)
    ).toBeDefined()
    expect(screen.queryByText(/No work tool sign-ins are connected yet/i)).toBeNull()
    expect(screen.getByRole('button', { name: /Sign in to GitHub/i })).toBeDefined()
  })

  test('labels missing work setup clearly instead of Unknown', async () => {
    useSettingsStore.setState({
      runtimeSettings: null,
      runtimeLoading: false,
      runtimeError: null,
    })

    render(<RuntimeSection />)

    expect(
      await screen.findByText('The Where agents run settings have not loaded yet.')
    ).toBeDefined()
    expect(
      screen.getByText(
        'Refresh this settings page to load the Where agents run settings. If they still do not load, ask an owner or admin to check agent setup.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/Where agents run could not load/i)).toBeNull()
    expect(screen.getByText('Load setup to choose a location')).toBeDefined()
    expect(screen.queryByText('Not set yet')).toBeNull()
    expect(screen.queryByText('Could not load work setup')).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('guides missing setup metrics toward the next action', async () => {
    agentApiMock.getCliAuthProxyStatus.mockResolvedValueOnce({
      ok: true,
      statuses: [],
    })
    orchestrationApiMock.getParticipants.mockResolvedValueOnce([])
    useSettingsStore.setState({
      runtimeSettings: {
        defaultRuntime: 'container',
        availableRuntimes: ['container'],
        defaultCliTool: 'codex',
        availableCliTools: ['codex'],
        cliToolDetails: [],
      },
    })

    render(<RuntimeSection />)

    expect(await screen.findByText('Check setup after tools finish.')).toBeDefined()
    expect(screen.getByText('Start an agent, then check again.')).toBeDefined()
    expect(screen.getByText(/No extra work tool sign-ins are needed/i)).toBeDefined()
    expect(
      screen.queryByText(/Sign in to a work tool before starting agents that need one/i)
    ).toBeNull()
    expect(screen.queryByText('No work tool status yet')).toBeNull()
    expect(screen.queryByText('No agent seen online yet')).toBeNull()
  })

  test('labels unknown agent location and tool values without exposing backend codes', async () => {
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

    expect((await screen.findAllByText('Check agent location')).length).toBeGreaterThan(0)
    expect(screen.getAllByText('Check work tool setup').length).toBeGreaterThan(0)
    expect(screen.queryByText(/future_runtime/i)).toBeNull()
    expect(screen.queryByText(/future_tool/i)).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('shows beginner guidance when work tool sign-in status cannot load', async () => {
    agentApiMock.getCliAuthProxyStatus.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<RuntimeSection />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(/work tool sign-in could not be checked/i)
    expect(alert).toHaveTextContent(/Forge could not connect while checking where agents run/i)
    expect(screen.queryByText(/failed to fetch/i)).toBeNull()
    expect(screen.queryByText(/app could not reach/i)).toBeNull()
    expect(screen.queryByText(/service is healthy/i)).toBeNull()
  })

  test('shows beginner guidance when agent online status cannot load', async () => {
    orchestrationApiMock.getParticipants.mockRejectedValueOnce(new Error('401 Unauthorized'))

    render(<RuntimeSection />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(/sign in again/i)
    expect(screen.queryByText(/code: 401/i)).toBeNull()
    expect(screen.queryByText(/Code:/i)).toBeNull()
    expect(screen.queryByText(/401 Unauthorized/)).toBeNull()
    expect(screen.queryByText(/service is healthy/i)).toBeNull()
  })

  test('shows beginner guidance when work tool sign-in cannot start', async () => {
    agentApiMock.startCliAuthProxyLogin.mockResolvedValueOnce({
      ok: false,
      error: '403 Forbidden',
    })

    render(<RuntimeSection />)

    await screen.findByTestId('runtime-launch-checklist')
    fireEvent.click(screen.getByRole('button', { name: /Sign in to GitHub/i }))

    expect(
      await screen.findByText(/do not have permission to change where agents run/i)
    ).toBeDefined()
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
      'Where agents run could not be saved. Choose an available agent location and work tool, then save again.'
    )
    expect(screen.queryByText(/Details: default CLI tool is not available/i)).toBeNull()
  })
})
