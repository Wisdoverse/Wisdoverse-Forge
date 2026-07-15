import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import '@app/i18n'
import { RuntimeSection } from '@app/features/settings/RuntimeSection'
import { useSettingsStore } from '@app/entities/settings'

const { agentApiMock, orchestrationApiMock } = vi.hoisted(() => ({
  agentApiMock: {
    getCliAuthProxyStatus: vi.fn(),
    getCliAuthProxyProviders: vi.fn(),
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

function expandDisclosure(container: HTMLElement) {
  const toggle = within(container)
    .getAllByRole('button')
    .find((button) => button.hasAttribute('aria-expanded'))
  expect(toggle).toBeDefined()
  if (toggle?.getAttribute('aria-expanded') === 'false') fireEvent.click(toggle)
  expect(toggle).toHaveAttribute('aria-expanded', 'true')
  return container
}

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
  agentApiMock.getCliAuthProxyProviders.mockResolvedValue({
    ok: true,
    providers: [
      { name: 'github', displayName: 'GitHub', cliTool: 'codex' },
      { name: 'gitlab', displayName: 'GitLab', cliTool: 'claude' },
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
    preferences: {},
    preferencesLoaded: true,
    preferencesLoading: false,
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
    preferences: null,
    preferencesLoaded: false,
    preferencesLoading: false,
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
    const nextStep = expandDisclosure(screen.getByTestId('runtime-next-step'))
    expect(readiness).toHaveClass('border-y', 'bg-transparent')
    expect(readiness.className).not.toContain('rounded-lg')
    expect(readiness.className).not.toMatch(/(^|\s)bg-white(\s|$)/)
    expect(nextStep).toHaveTextContent('Next step')
    expect(nextStep).toHaveTextContent('Work tools ready')
    expect(nextStep).toHaveTextContent('What success looks like: This item changes to Ready.')
    expect(nextStep).not.toHaveTextContent('Success:')
    expect(screen.getByText('Before sending Tasks that change project files')).toBeDefined()
    expect(screen.queryByText('Before sending file work')).toBeNull()
    expect(screen.queryByText('Before assigning work')).toBeNull()
    expect(screen.getByText('2/4 ready')).toBeDefined()
    expect(within(readiness).getByText('Finish where agents work')).toBeDefined()
    expect(
      screen.getByText(
        /Forge can run agents in 2 places and use 2 tools that can change project files, such as Claude or Codex/i
      )
    ).toBeDefined()
    expect(screen.getByText(/1 tool sign-in is connected\. 1 agent is online/i)).toBeDefined()
    expect(
      within(readiness).queryByText(new RegExp('agent locations\\s+available', 'i'))
    ).toBeNull()
    expect(screen.queryByText(/work places available/i)).toBeNull()
    expect(
      screen.queryByText(new RegExp('tools like Claude or Codex\\s+available', 'i'))
    ).toBeNull()
    expect(screen.queryByText(/seen online/i)).toBeNull()
    expect(
      screen.getAllByText(/change project files, run checks, or show live progress/i).length
    ).toBeGreaterThan(0)
    expect(screen.queryByText(/commands, or live work access/i)).toBeNull()
    expect(
      screen.getByText(
        'Choose Project files for the simplest shared project changes. Choose This computer only when work needs files and tools on this computer.'
      )
    ).toBeDefined()
    const runtimeSettingsFrame = screen
      .getByText(
        'Choose Project files for the simplest shared project changes. Choose This computer only when work needs files and tools on this computer.'
      )
      .closest('div')?.parentElement?.parentElement
    expect(runtimeSettingsFrame).toHaveClass('border-y', 'bg-transparent')
    expect(runtimeSettingsFrame?.className).not.toContain('rounded-card')
    expect(runtimeSettingsFrame?.className).not.toMatch(/(^|\s)bg-white(\s|$)/)
    expect(screen.queryByText(/machine should join as an agent/i)).toBeNull()
    expect(screen.queryByText(new RegExp(['unless', 'owner', 'tells'].join('.*'), 'i'))).toBeNull()
    expect(screen.getByText('Available file locations')).toBeDefined()
    expect(screen.getByText('Choices shown for where project files open')).toBeDefined()
    expect(screen.queryByText('Places that can open project files')).toBeNull()
    expect(screen.getAllByText(/Work tools/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/tool install status/i)).toBeNull()
    expect(screen.getAllByText('Project files').length).toBeGreaterThan(0)
    expect(screen.queryByText('Managed workspace')).toBeNull()
    expect(screen.getAllByText('Simple chat agent').length).toBeGreaterThan(0)
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
    expect(screen.getByText('Install this work tool')).toBeDefined()
    expect(screen.queryByText('Needs attention')).toBeNull()
    expect(screen.getByText('Install this tool')).toBeDefined()
    expect(screen.queryByText('Setup needed')).toBeNull()
    expect(
      within(screen.getByTestId('runtime-cli-versions')).getByText('Check again')
    ).toBeDefined()
    expect(screen.queryByText('Check setup')).toBeNull()
    expect(screen.queryByText('check tool')).toBeNull()
    expect(screen.getByText('Installed and ready')).toBeDefined()
    expect(screen.getAllByText(/code tool sign-ins/i).length).toBeGreaterThan(0)
    expect(screen.getByText(/1\/2 code tool sign-ins ready/i)).toBeDefined()
    expect(screen.getByText(/Choose Sign in to GitHub/i)).toBeDefined()
    expect(screen.queryByText(/No work tool sign-in saved/i)).toBeNull()
    expect(screen.getAllByRole('button', { name: /Sign in to GitHub/i }).length).toBeGreaterThan(0)
    expect(screen.getByText(/browser login page opens/i)).toBeDefined()
    expect(screen.queryByRole('button', { name: /^Sign in$/i })).toBeNull()
    expect(screen.getAllByRole('button', { name: /Check again/i }).length).toBeGreaterThan(0)
    expect(
      screen.queryByRole('button', { name: new RegExp(['Check', 'status'].join(' '), 'i') })
    ).toBeNull()
    expect(screen.queryByRole('button', { name: /^Refresh$/i })).toBeNull()
    expect(screen.queryByText('Needs action')).toBeNull()
    expect(screen.queryByText(/still need attention/i)).toBeNull()
    expect(screen.getAllByText('Check before use').length).toBeGreaterThan(0)
    expect(screen.queryByText('Needs setup')).toBeNull()

    fireEvent.click(screen.getAllByRole('button', { name: /Sign in to GitHub/i })[0])

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
    agentApiMock.getCliAuthProxyProviders.mockResolvedValueOnce({
      ok: true,
      providers: [{ name: 'github', displayName: 'GitHub', cliTool: 'codex' }],
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
    expandDisclosure(screen.getByTestId('runtime-next-step'))
    expect(
      screen.getByText(
        /Forge can run agents in 1 place and use 1 tool that can change project files, such as Claude or Codex/i
      )
    ).toBeDefined()
    expect(screen.getByText(/1 tool sign-in is connected\. 1 agent is online/i)).toBeDefined()
    expect(screen.getByTestId('runtime-next-step')).toHaveTextContent('Ready to give agents work')
    expect(screen.getByTestId('runtime-next-step')).toHaveTextContent('Where project files open')
    expect(screen.getByTestId('runtime-next-step')).toHaveTextContent(
      'What success looks like: Open Agents, create or select an agent, then send a task from Tasks.'
    )
    expect(screen.getByTestId('runtime-next-step')).not.toHaveTextContent('send work from Tasks')
    expect(screen.getByTestId('runtime-next-step')).not.toHaveTextContent('assign work')
    expect(screen.getByTestId('runtime-next-step')).not.toHaveTextContent('Success:')
    expect(screen.queryByRole('button', { name: /Sign in to GitHub/i })).toBeNull()
    expect(screen.getByText(/1\/1 work tools are ready/i)).toBeDefined()
    expect(screen.getByText(/1\/1 code tool sign-ins ready/i)).toBeDefined()
    expect(screen.getAllByText(/agent online status/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/agent tools are checked/i)).toBeNull()
    expect(screen.queryByText(/agent check-ins/i)).toBeNull()
  })

  test('treats installed work tools without a shown version as ready', async () => {
    agentApiMock.getCliAuthProxyStatus.mockResolvedValueOnce({
      ok: true,
      statuses: [],
    })
    agentApiMock.getCliAuthProxyProviders.mockResolvedValueOnce({
      ok: true,
      providers: [],
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
            image: 'agentforge-agent',
            imagePresent: true,
            versionSource: 'not-reported',
          },
        ],
      },
    })

    render(<RuntimeSection />)

    expect(await screen.findByText('4/4 ready')).toBeDefined()
    expandDisclosure(screen.getByTestId('runtime-next-step'))
    expect(screen.getByTestId('runtime-next-step')).toHaveTextContent('Ready to give agents work')
    expect(screen.getAllByText(/1\/1 work tools ready/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Version not shown yet')).toBeDefined()
    expect(screen.getByText('Installed and ready')).toBeDefined()
    expect(screen.queryByText(/without a version yet/i)).toBeNull()
    expect(screen.queryByText(/finish setting up the tools without a version/i)).toBeNull()
  })

  test('does not expose internal work tool image names', async () => {
    render(<RuntimeSection />)

    await screen.findByTestId('runtime-launch-checklist')
    expect(screen.queryByTitle('agentforge-agent:codex')).toBeNull()
    expect(screen.queryByTitle('agentforge-agent:claude')).toBeNull()
    expect(document.body.innerHTML).not.toContain('agentforge-agent:')
  })

  test('tells users to sign in before starting agents when no code tool sign-ins are connected', async () => {
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
      screen.getByText(
        /Sign in to a code tool before starting agents that need to change project files/i
      )
    ).toBeDefined()
    expect(screen.queryByText(/No work tool sign-ins are connected yet/i)).toBeNull()
    expect(screen.getAllByText(/Choose Sign in to GitHub/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/No work tool sign-in saved/i)).toBeNull()
    expect(screen.getAllByRole('button', { name: /Sign in to GitHub/i }).length).toBeGreaterThan(0)
    expect(screen.queryByRole('button', { name: /^Sign in$/i })).toBeNull()
  })

  test('keeps the code tool sign-in entry visible when status rows have not been created yet', async () => {
    const openSpy = vi.spyOn(window, 'open').mockImplementation(() => null)
    agentApiMock.getCliAuthProxyStatus.mockResolvedValueOnce({
      ok: true,
      statuses: [],
    })
    agentApiMock.getCliAuthProxyProviders.mockResolvedValueOnce({
      ok: true,
      providers: [{ name: 'openai', displayName: 'OpenAI (Codex)', cliTool: 'codex' }],
    })

    render(<RuntimeSection focus="sign-ins" />)

    expect(await screen.findByRole('heading', { name: 'Sign in to code tools' })).toBeDefined()
    expect(screen.queryByRole('heading', { name: 'Code tool sign-in' })).toBeNull()
    expect(
      screen.getByText('Sign in to Codex or another tool before agents work on project files.')
    ).toBeDefined()
    const signInEntry = await screen.findByTestId('runtime-sign-in-entry')
    expandDisclosure(signInEntry)
    expect(signInEntry).toHaveTextContent('Start here when Codex asks you to sign in')
    expect(screen.getByTestId('runtime-sign-in-entry')).not.toHaveTextContent(
      'Code tool sign-in starts here'
    )
    expect(screen.getByTestId('runtime-sign-in-entry')).toHaveTextContent(
      'Use this page when Codex or another code tool asks you to sign in.'
    )
    expect(screen.getByTestId('runtime-sign-in-entry')).toHaveTextContent(
      'For Codex, choose Sign in to OpenAI (Codex)'
    )
    expect(screen.getByTestId('runtime-sign-in-entry')).toHaveTextContent(
      'ask an owner or admin to check Codex sign-in in Settings'
    )
    expect(screen.getByTestId('runtime-sign-in-entry')).not.toHaveTextContent(
      'check work tool sign-ins'
    )
    expect(screen.queryByText(/Start Codex sign-in here/i)).toBeNull()
    expect(screen.queryByRole('heading', { name: 'Codex sign-in' })).toBeNull()
    expect(screen.queryByText(/asks for login/i)).toBeNull()
    expect(screen.queryByText(/Sign in to Codex CLI and work tools/i)).toBeNull()
    expect(screen.queryByText('Codex and work tool sign-in')).toBeNull()
    expect(screen.getByText('OpenAI (Codex)')).toBeDefined()
    expect(screen.getAllByText('Codex').length).toBeGreaterThan(0)
    expect(screen.getAllByText(/Choose Sign in to OpenAI \(Codex\)/i).length).toBeGreaterThan(0)
    expect(
      screen.getAllByRole('button', { name: /Sign in to OpenAI \(Codex\)/i }).length
    ).toBeGreaterThan(0)
    expect(screen.queryByRole('button', { name: /^Sign in$/i })).toBeNull()

    fireEvent.click(screen.getAllByRole('button', { name: /Sign in to OpenAI \(Codex\)/i })[0])

    await waitFor(() => expect(agentApiMock.startCliAuthProxyLogin).toHaveBeenCalledWith('openai'))
    expect(openSpy).toHaveBeenCalledWith(
      'https://auth.example.test/start',
      '_blank',
      'noopener,noreferrer'
    )
  })

  test('matches the Codex sign-in instruction to the shown provider name', async () => {
    render(<RuntimeSection focus="sign-ins" />)

    const signInEntry = await screen.findByTestId('runtime-sign-in-entry')
    expandDisclosure(signInEntry)
    expect(signInEntry).toHaveTextContent('For Codex, choose Sign in to GitHub')
    expect(screen.getByTestId('runtime-sign-in-entry')).not.toHaveTextContent(
      'choose Sign in next to OpenAI (Codex)'
    )
    expect(screen.getAllByRole('button', { name: /Sign in to GitHub/i }).length).toBeGreaterThan(0)
  })

  test('explains how to recover when the browser blocks the sign-in page', async () => {
    vi.spyOn(window, 'open').mockImplementation(() => null)

    render(<RuntimeSection />)

    await screen.findByTestId('runtime-launch-checklist')
    fireEvent.click(screen.getAllByRole('button', { name: /Sign in to GitHub/i })[0])

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Allow pop-ups for this site, then choose Sign in to GitHub again.'
    )
    expect(alert).toHaveTextContent('The GitHub browser sign-in page did not open.')
    expect(alert).not.toHaveTextContent('window.open')
    expect(alert).not.toHaveTextContent('popup blocker')
  })

  test('labels missing work setup clearly instead of Unknown', async () => {
    useSettingsStore.setState({
      runtimeSettings: null,
      runtimeLoading: false,
      runtimeError: null,
    })

    render(<RuntimeSection />)

    const loadGuidance = await screen.findAllByText(
      'Open Settings, then open Where agents work. If it still does not load, ask an owner or admin to check Where agents work in Settings.'
    )
    expect(loadGuidance.length).toBeGreaterThanOrEqual(1)
    expect(screen.getAllByRole('button', { name: /Check again/i }).length).toBeGreaterThanOrEqual(1)
    expect(screen.queryByText(/settings have not loaded yet/i)).toBeNull()
    expect(screen.queryByText(/check setup\. if/i)).toBeNull()
    expect(screen.queryByText(/Agent work setup could not load/i)).toBeNull()
    expect(screen.getByText('Load setup to choose where project files open')).toBeDefined()
    expect(screen.queryByText('Not set yet')).toBeNull()
    expect(screen.queryByText('Could not load work setup')).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('guides missing setup metrics toward the next action', async () => {
    agentApiMock.getCliAuthProxyStatus.mockResolvedValueOnce({
      ok: true,
      statuses: [],
    })
    agentApiMock.getCliAuthProxyProviders.mockResolvedValueOnce({
      ok: true,
      providers: [],
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
    expandDisclosure(await screen.findByTestId('runtime-next-step'))

    expect(
      await screen.findAllByText(
        'Check again after tools finish. If this stays here, ask an owner to finish adding the tools.'
      )
    ).toHaveLength(3)
    expect(
      screen.getAllByText(
        'Open Agents and make sure one agent shows Ready, then choose Check again.'
      ).length
    ).toBe(2)
    expect(screen.getByText(/No extra tool sign-ins are needed/i)).toBeDefined()
    expect(screen.getAllByText(/Open Agents and make sure one agent shows Ready/i).length).toBe(3)
    expect(screen.queryByText(/wake an agent/i)).toBeNull()
    expect(
      screen.queryByText(
        /Sign in to a code tool before starting agents that need to change project files/i
      )
    ).toBeNull()
    expect(screen.queryByText('No work tool status yet')).toBeNull()
    expect(screen.queryByText('No agent seen online yet')).toBeNull()
    expect(screen.queryByText(/No work tool setup status yet/i)).toBeNull()
    expect(screen.queryByText(/No agent has been seen online yet/i)).toBeNull()
    expect(screen.queryByText(/no agents are online yet/i)).toBeNull()
  })

  test('labels unknown project-file and tool values without exposing backend codes', async () => {
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

    expect((await screen.findAllByText('Check where files open')).length).toBeGreaterThan(0)
    expect(screen.getAllByText('Check work tool').length).toBeGreaterThan(0)
    expect(screen.queryByText(/future_runtime/i)).toBeNull()
    expect(screen.queryByText(/future_tool/i)).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('shows beginner guidance when code tool sign-in status cannot load', async () => {
    agentApiMock.getCliAuthProxyStatus.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<RuntimeSection />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(/code tool sign-in could not be checked/i)
    expect(alert).toHaveTextContent(
      /Forge could not connect while checking the Codex sign-in page/i
    )
    expect(screen.getByText(/Choose Check again to check Codex sign-in/i)).toBeDefined()
    expect(screen.queryByText(/check work tool sign-ins/i)).toBeNull()
    expect(screen.queryByText(/^Code tool sign-ins could not be checked/i)).toBeNull()
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
    expect(screen.getByText(/Choose Check again to check agent online status/i)).toBeDefined()
    expect(screen.queryByText(/^Agent online status could not be checked/i)).toBeNull()
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
    fireEvent.click(screen.getAllByRole('button', { name: /Sign in to GitHub/i })[0])

    expect(await screen.findByText(/do not have permission to change Codex sign-in/i)).toBeDefined()
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
      'Choose where project files open and a work tool, then save again. Where agents work could not be saved.'
    )
    expect(screen.queryByText(/Details: default CLI tool is not available/i)).toBeNull()
  })
})
