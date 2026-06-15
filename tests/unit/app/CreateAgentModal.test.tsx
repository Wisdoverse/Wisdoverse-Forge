import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { CreateAgentModal } from '@app/features/agents/CreateAgentModal'
import { useAgentsStore } from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { agentGroupApi } from '@app/entities/agent-group'

vi.mock('@app/entities/agent-group', () => ({
  agentGroupApi: {
    getGroups: vi.fn().mockResolvedValue([]),
    createGroup: vi.fn().mockResolvedValue({
      id: 'group-new',
      name: 'Default Task Queue',
      projectId: 'p1',
    }),
  },
}))

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

beforeEach(() => {
  useAgentsStore.getState().reset()
  useNavigationStore.getState().reset()
  useSettingsStore.setState({
    providers: [],
    providersLoading: false,
    providersError: null,
  })
  vi.mocked(agentGroupApi.getGroups).mockResolvedValue([])
  vi.mocked(agentGroupApi.createGroup).mockResolvedValue({
    id: 'group-new',
    name: 'Default Task Queue',
    projectId: 'p1',
  })
  useAgentsStore.setState({
    createModalOpen: true,
    loading: false,
    error: null,
  })
})

describe('CreateAgentModal', () => {
  const previousCliInstallCopy = new RegExp(
    ['where', 'the', 'CLI', 'is', 'installed'].join('\\s+'),
    'i'
  )
  const previousManualConnectionCopy = new RegExp(
    ['manual', 'connection', 'setup'].join('\\s+'),
    'i'
  )

  test('renders managed workspace fields by default', () => {
    render(<CreateAgentModal />)

    expect(screen.getByRole('heading', { name: 'Create an agent' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: /managed workspace/i })).toBeChecked()
    expect(screen.getByTestId('agent-runtime-fit')).toBeInTheDocument()
    expect(screen.getByText('Start with a role')).toBeInTheDocument()
    expect(screen.getByText('Fills in the agent name')).toBeInTheDocument()
    expect(screen.getByText('Updates the work and checks it')).toBeInTheDocument()
    expect(screen.queryByText('Builds changes and checks them')).toBeNull()
    expect(screen.getAllByText(/claude in a managed workspace/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Project files included')).toBeInTheDocument()
    expect(screen.getByText('Agent location')).toBeInTheDocument()
    expect(screen.getByText('Check Where agents run in Settings')).toBeInTheDocument()
    expect(screen.getByText('Can edit files')).toBeInTheDocument()
    expect(screen.queryByText(/workspace must be ready/i)).toBeNull()
    expect(screen.queryByText('File work')).toBeNull()
    expect(screen.getByRole('combobox', { name: /^work tool$/i })).toBeInTheDocument()
    expect(screen.getByLabelText(/work folder/i)).toBeInTheDocument()
    expect(screen.getByText(/keep the suggested folder/i)).toBeInTheDocument()
    expect(screen.getByText(/new tasks start from the primary project/i)).toBeInTheDocument()
    expect(screen.queryByText(/use \/workspace unless/i)).toBeNull()
    expect(screen.queryByText(/default task context/i)).toBeNull()
    expect(screen.getAllByText(/primary project/i).length).toBeGreaterThan(0)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/choose a project first/i)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(
      /select a project in the sidebar/i
    )
    const review = screen.getByTestId('agent-create-review')
    expect(within(review).getByText('Before you create')).toBeInTheDocument()
    expect(within(review).getByText(/claude in a managed workspace/i)).toBeInTheDocument()
    expect(within(review).getByText('No project selected yet')).toBeInTheDocument()
    expect(
      within(review).getByText('Choose a project later before assigning tasks.')
    ).toBeInTheDocument()
    expect(
      within(review).getByText('Start the agent, then send one small task from Tasks.')
    ).toBeInTheDocument()
    expect(screen.queryByLabelText(/^ai service$/i)).toBeNull()
    expect(screen.queryByLabelText(/^ai model$/i)).toBeNull()
    expect(screen.queryByText(/Name seeds CLI agents/i)).toBeNull()
    expect(screen.queryByLabelText(/^provider$/i)).toBeNull()
    expect(screen.queryByLabelText(/^model$/i)).toBeNull()
  })

  test('shows selected project as the primary project context', () => {
    useNavigationStore.setState({
      selectedProjectId: 'p1',
      projects: {
        t1: [
          {
            id: 'p1',
            teamId: 't1',
            workspaceId: 'w1',
            name: 'Platform',
            slug: 'platform',
            color: '#007AFF',
            description: '',
          },
        ],
      },
    })

    render(<CreateAgentModal />)

    expect(screen.getAllByText('Platform').length).toBeGreaterThan(0)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/project ready/i)
    expect(screen.getByText(/tasks default to this project/i)).toBeInTheDocument()
    const review = screen.getByTestId('agent-create-review')
    expect(within(review).getByText('Platform')).toBeInTheDocument()
    expect(
      within(review).getByText(
        'Create a task queue here when you want new tasks to wait in one place.'
      )
    ).toBeInTheDocument()
  })

  test('submits selected project workspace as the execution boundary', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)
    useNavigationStore.setState({
      selectedProjectId: 'p1',
      projects: {
        t1: [
          {
            id: 'p1',
            teamId: 't1',
            workspaceId: 'w1',
            name: 'Platform',
            slug: 'platform',
            color: '#007AFF',
            description: '',
          },
        ],
      },
    })

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'CLI Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      workspaceId: 'w1',
      projectId: 'p1',
    })
  })

  test('guides users to name the agent before creating it', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Name this agent before creating it.'
    )
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('creates and selects a task queue for the selected project', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)
    useNavigationStore.setState({
      selectedProjectId: 'p1',
      projects: {
        t1: [
          {
            id: 'p1',
            teamId: 't1',
            workspaceId: 'w1',
            name: 'Platform',
            slug: 'platform',
            color: '#007AFF',
            description: '',
          },
        ],
      },
    })

    render(<CreateAgentModal />)
    expect(
      screen.getByText(/starter queue for this project so new tasks have a clear place to wait/i)
    ).toBeInTheDocument()
    fireEvent.click(await screen.findByRole('button', { name: /create task queue/i }))

    await waitFor(() =>
      expect(agentGroupApi.createGroup).toHaveBeenCalledWith({
        projectId: 'p1',
        name: 'Default Task Queue',
        description:
          'Starter queue for this project. New tasks wait here until an agent can take them.',
      })
    )
    expect(screen.getByRole('combobox', { name: /task queue/i })).toHaveValue('group-new')
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/project ready/i)
    expect(
      screen.getByText(/new tasks can wait in this queue until an available agent can take them/i)
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('agent-create-review')).getByText('Default Task Queue')
    ).toBeInTheDocument()
    expect(screen.queryByText(/work a place to wait until this agent can take it/i)).toBeNull()
    expect(screen.queryByText(new RegExp('board\\s+tasks', 'i'))).toBeNull()

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'CLI Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      workspaceId: 'w1',
      projectId: 'p1',
      groupId: 'group-new',
    })
  })

  test('hides raw task queue creation errors while creating a default queue', async () => {
    vi.mocked(agentGroupApi.createGroup).mockRejectedValueOnce(
      new Error('HTTP 500: database unavailable')
    )
    useNavigationStore.setState({
      selectedProjectId: 'p1',
      projects: {
        t1: [
          {
            id: 'p1',
            teamId: 't1',
            workspaceId: 'w1',
            name: 'Platform',
            slug: 'platform',
            color: '#007AFF',
            description: '',
          },
        ],
      },
    })

    render(<CreateAgentModal />)
    fireEvent.click(await screen.findByRole('button', { name: /create task queue/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Task queue was not created.')
    expect(alert).toHaveTextContent('ask an owner or admin to check task queue setup')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('database unavailable')
  })

  test('switching to simple chat agent hides work tool fields and shows AI service details', () => {
    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))

    expect(screen.getByText('Fills in name and instructions')).toBeInTheDocument()
    expect(screen.getAllByText(/anthropic simple chat agent/i).length).toBeGreaterThan(0)
    expect(screen.getByText(/questions, planning, writing, and review/i)).toBeInTheDocument()
    expect(screen.getByText(/does not open project files/i)).toBeInTheDocument()
    expect(screen.getByText('Check AI service in Settings')).toBeInTheDocument()
    expect(screen.queryByText(/ai service must be checked/i)).toBeNull()
    expect(screen.queryByRole('combobox', { name: /^work tool$/i })).toBeNull()
    expect(screen.queryByLabelText(/work folder/i)).toBeNull()
    expect(screen.getByLabelText(/^ai service$/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/^ai model$/i)).toBeInTheDocument()
    expect(screen.getByText(/keep the suggested AI model/i)).toBeInTheDocument()
    const review = screen.getByTestId('agent-create-review')
    expect(
      within(review).getByText(
        'Ask a first question or assign review work that does not need files.'
      )
    ).toBeInTheDocument()
    expect(
      within(review).getByText('Ready for chat and review after the AI service is connected.')
    ).toBeInTheDocument()
    expect(screen.queryByLabelText(/^model name$/i)).toBeNull()
    expect(screen.queryByLabelText(/^provider$/i)).toBeNull()
    expect(screen.queryByLabelText(/^model$/i)).toBeNull()
  })

  test('updates runtime fit when the operator changes runtime choices', async () => {
    render(<CreateAgentModal />)

    fireEvent.change(screen.getByRole('combobox', { name: /^work tool$/i }), {
      target: { value: 'codex' },
    })
    expect(screen.getAllByText(/codex in a managed workspace/i).length).toBeGreaterThan(0)

    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    expect(screen.getAllByText(/codex on this computer/i).length).toBeGreaterThan(0)
    expect(
      screen.getByText(/files and commands on your computer\. Forge still manages the agent here/i)
    ).toBeInTheDocument()
    expect(
      screen.getByText(/After setup, Forge still manages this agent here/i)
    ).toBeInTheDocument()
    expect(screen.getAllByText(/tasks, status, and task history/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/Forge gives it tasks/i)).toBeNull()
    expect(screen.getByText('Run setup command on this computer')).toBeInTheDocument()
    expect(screen.getByText('Uses this computer')).toBeInTheDocument()
    const localReview = screen.getByTestId('agent-create-review')
    expect(
      within(localReview).getByText(
        'Run the setup command on this computer and keep that window open.'
      )
    ).toBeInTheDocument()
    expect(
      within(localReview).getByText(
        'Forge creates the agent, then shows a setup command for this computer.'
      )
    ).toBeInTheDocument()
    expect(
      screen.queryByText(new RegExp(['work tool', 'installed', 'your computer'].join('.*'), 'i'))
    ).toBeNull()
    expect(screen.queryByText(new RegExp(['Local', 'work'].join('\\s+')))).toBeNull()

    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.change(screen.getByLabelText(/^ai service$/i), { target: { value: 'google' } })

    await waitFor(() => {
      expect(screen.getAllByText(/google simple chat agent/i).length).toBeGreaterThan(0)
    })
    expect(screen.getByText('Chat-only AI service')).toBeInTheDocument()
    expect(screen.getByText('Check AI service in Settings')).toBeInTheDocument()
    expect(
      screen.queryByText(new RegExp(['AI service', 'must be checked'].join('.*'), 'i'))
    ).toBeNull()
  })

  test('enrolls an agent on this computer and shows the setup command', async () => {
    const enrollLocalAgent = vi.fn().mockResolvedValue({
      ok: true,
      agent: {
        id: 'a-local',
        name: 'Laptop Worker',
        status: 'offline',
        createdAt: Date.now(),
        lastActivity: Date.now(),
        cliTool: 'codex',
        runtimeId: 'host-a-local',
      },
      enrollment: {
        agentId: 'a-local',
        runtimeId: 'host-a-local',
        cliTool: 'codex',
        env: { AGENT_ID: 'a-local' },
        shellExports: "export AGENT_ID='a-local'\nagentforge-sidecar",
        sidecarCommand: 'agentforge-sidecar',
        serverUrl: 'https://forge.example.com',
      },
    })
    useAgentsStore.setState({ enrollLocalAgent } as never)

    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    fireEvent.change(screen.getByRole('combobox', { name: /work tool on this computer/i }), {
      target: { value: 'codex' },
    })
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Laptop Worker' } })
    fireEvent.change(screen.getByLabelText(/folder on this computer/i), {
      target: { value: '/Users/me/project' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(enrollLocalAgent).toHaveBeenCalledTimes(1))
    expect(enrollLocalAgent.mock.calls[0][0]).toMatchObject({
      name: 'Laptop Worker',
      cliTool: 'codex',
      cwd: '/Users/me/project',
    })
    expect(await screen.findByLabelText(/setup command/i)).toHaveValue(
      "export AGENT_ID='a-local'\nagentforge-sidecar"
    )
    expect(screen.getByText('Connect this computer')).toBeInTheDocument()
    expect(screen.getByText(/agent managed by forge/i)).toBeInTheDocument()
    expect(screen.getByText(/keep it running so forge can manage this agent/i)).toBeInTheDocument()
    expect(screen.getByText('1. Copy this setup command.')).toBeInTheDocument()
    expect(screen.getByText(/paste it into the terminal app/i)).toBeInTheDocument()
    expect(
      screen.getByText(/changes from Not connected to Ready on the Agents page/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/come back to Forge, open Agents/i)).toBeInTheDocument()
    expect(screen.queryByText(previousCliInstallCopy)).toBeNull()
    expect(screen.queryByText(previousManualConnectionCopy)).toBeNull()
    expect(screen.queryByText(new RegExp(['local', 'agent', 'join'].join('.*'), 'i'))).toBeNull()
  })

  test('shows the setup command with an OS toggle when the server mints a join code', async () => {
    const joinCommand =
      'curl -fsSL https://forge.example.com/api/v1/agents/local-join/script | sh -s -- --code afj_test'
    const joinCommandPowershell =
      "$env:AGENTFORGE_JOIN_CODE = 'afj_test'; irm https://forge.example.com/api/v1/agents/local-join/script.ps1 | iex"
    const enrollLocalAgent = vi.fn().mockResolvedValue({
      ok: true,
      agent: {
        id: 'a-local',
        name: 'Laptop Worker',
        status: 'offline',
        createdAt: Date.now(),
        lastActivity: Date.now(),
        cliTool: 'codex',
        runtimeId: 'host-a-local',
      },
      enrollment: {
        agentId: 'a-local',
        runtimeId: 'host-a-local',
        cliTool: 'codex',
        env: { AGENT_ID: 'a-local' },
        shellExports: "export AGENT_ID='a-local'\nagentforge-sidecar",
        sidecarCommand: 'agentforge-sidecar',
        serverUrl: 'https://forge.example.com',
        joinCode: 'afj_test',
        joinCommand,
        joinCommandPowershell,
      },
    })
    useAgentsStore.setState({ enrollLocalAgent } as never)

    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Laptop Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    // The setup command leads; the pasted command tracks the OS toggle.
    const oneLiner = await screen.findByLabelText(/setup command/i)
    expect(oneLiner).toHaveValue(joinCommand)
    expect(screen.getByText(/lets forge assign tasks to this agent/i)).toBeInTheDocument()
    expect(screen.getByTestId('local-agent-paste-hint')).toHaveTextContent(
      'Open Terminal on macOS or your Linux terminal, then paste this command.'
    )
    expect(screen.getByText('1. Copy this setup command.')).toBeInTheDocument()
    expect(screen.getByText(/paste it into terminal or powershell/i)).toBeInTheDocument()
    expect(
      screen.getByText(/changes from Not connected to Ready on the Agents page/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/come back to Forge, open Agents/i)).toBeInTheDocument()
    expect(screen.queryByText(/shows online/i)).toBeNull()
    expect(screen.queryByText(/agent fleet/i)).toBeNull()
    expect(screen.getByRole('group', { name: /computer type/i })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /windows/i }))
    expect(oneLiner).toHaveValue(joinCommandPowershell)
    expect(screen.getByTestId('local-agent-paste-hint')).toHaveTextContent(
      'Open PowerShell on Windows, then paste this command.'
    )

    // Backup values stay available without exposing advanced connection jargon.
    expect(screen.getByText(/if the setup command does not work/i)).toBeInTheDocument()
    const backupHelp = screen.getByText(/backup setup values/i)
    expect(backupHelp).toBeInTheDocument()
    expect(backupHelp.textContent).toMatch(/same Terminal or PowerShell window/)
    expect(backupHelp.textContent).not.toMatch(/advanced/i)
    expect(backupHelp.textContent).not.toMatch(/sidecar/i)
    expect(screen.queryByText(previousManualConnectionCopy)).toBeNull()
    expect(screen.getByRole('button', { name: /copy backup setup/i })).toBeInTheDocument()
    expect(screen.getByLabelText(/backup setup values/i)).toHaveValue(
      "export AGENT_ID='a-local'\nagentforge-sidecar"
    )
  })

  test('uses a beginner fallback name when the setup response has no agent name', async () => {
    const enrollLocalAgent = vi.fn().mockResolvedValue({
      ok: true,
      agent: {
        id: 'a-local',
        status: 'offline',
        createdAt: Date.now(),
        lastActivity: Date.now(),
        cliTool: 'codex',
        runtimeId: 'host-a-local',
      },
      enrollment: {
        agentId: 'a-local',
        runtimeId: 'host-a-local',
        cliTool: 'codex',
        shellExports: "export AGENT_ID='a-local'\nagentforge-sidecar",
      },
    })
    useAgentsStore.setState({ enrollLocalAgent } as never)

    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Laptop Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    expect(await screen.findByText('This computer agent')).toBeInTheDocument()
    expect(screen.queryByText(new RegExp(['Local', 'agent'].join(' '), 'i'))).toBeNull()
  })

  test('defaults to simple chat agent when a verified provider exists', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-1',
          provider: 'openai',
          displayName: 'OpenAI',
          model: 'gpt-5.5',
          priority: 1,
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        },
      ],
    })

    render(<CreateAgentModal />)

    expect(screen.getByRole('radio', { name: /simple chat agent/i })).toBeChecked()
    expect(screen.queryByRole('combobox', { name: /^work tool$/i })).toBeNull()
    expect(screen.getByLabelText(/^ai service$/i)).toHaveValue('openai')
    expect(screen.getByLabelText(/^ai model$/i)).toHaveValue('gpt-5.5')

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      kind: 'provider',
      name: 'Provider Worker',
      provider: 'openai',
      model: 'gpt-5.5',
    })
  })

  test('switching provider seeds the matching default model', async () => {
    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.change(screen.getByLabelText(/^ai service$/i), { target: { value: 'openai' } })

    await waitFor(() => {
      expect(screen.getByLabelText(/^ai model$/i)).toHaveValue('gpt-4o')
    })
  })

  test('lists China-region providers and seeds the Zhipu GLM default model', async () => {
    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))

    const providerSelect = screen.getByLabelText(/^ai service$/i)
    expect(within(providerSelect).getByRole('option', { name: 'Zhipu GLM' })).toBeInTheDocument()
    expect(
      within(providerSelect).getByRole('option', { name: 'Zhipu GLM Coding Plan' })
    ).toBeInTheDocument()
    expect(
      within(providerSelect).getByRole('option', { name: 'Moonshot Kimi' })
    ).toBeInTheDocument()
    expect(
      within(providerSelect).getByRole('option', { name: 'Alibaba Qwen (DashScope)' })
    ).toBeInTheDocument()
    expect(
      within(providerSelect).getByRole('option', { name: 'Tencent Hunyuan' })
    ).toBeInTheDocument()

    fireEvent.change(providerSelect, { target: { value: 'zhipu' } })

    await waitFor(() => {
      expect(screen.getByLabelText(/^ai model$/i)).toHaveValue('glm-4.7')
    })
  })

  test('submits cli kind without provider/model fields', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'CLI Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    const payload = createAgent.mock.calls[0][0]
    expect(payload).toMatchObject({
      kind: 'cli',
      name: 'CLI Worker',
      cliTool: 'claude',
      cwd: '/workspace',
    })
    expect(payload).not.toHaveProperty('provider')
    expect(payload).not.toHaveProperty('model')
  })

  test('submits provider kind without cliTool', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    const payload = createAgent.mock.calls[0][0]
    expect(payload).toMatchObject({
      kind: 'provider',
      name: 'Provider Worker',
      provider: 'anthropic',
      model: 'claude-sonnet-4-6',
    })
    expect(payload).not.toHaveProperty('cliTool')
  })

  test('whitespace-only name shows the same beginner-safe error', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: '   ' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Name this agent before creating it.')
    )
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('provider kind with an empty model shows a visible error', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.change(screen.getByLabelText(/^ai model$/i), { target: { value: '' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(
        'Choose an AI service and AI model before creating this agent.'
      )
    )
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('a second failed submit with the same message scrolls the banner again', async () => {
    const scrollSpy = vi
      .spyOn(Element.prototype, 'scrollIntoView')
      .mockImplementation(() => undefined)
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    const submit = screen.getByRole('button', { name: /^create agent$/i })

    fireEvent.click(submit)
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Name this agent before creating it.')
    )
    const callsAfterFirst = scrollSpy.mock.calls.length
    expect(callsAfterFirst).toBeGreaterThan(0)

    fireEvent.click(submit)
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(callsAfterFirst))
    expect(createAgent).not.toHaveBeenCalled()
    scrollSpy.mockRestore()
  })

  test('clipboard failure on the join screen shows a manual-copy message', async () => {
    const enrollLocalAgent = vi.fn().mockResolvedValue({
      ok: true,
      agent: { id: 'a1', name: 'Local Worker' },
      enrollment: { shellExports: 'export AGENTFORGE_NATS_URL=nats://example:4222' },
    })
    useAgentsStore.setState({
      enrollLocalAgent: async (opts: never) => {
        const result = await enrollLocalAgent(opts)
        return result
      },
    } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Local Worker' } })
    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    const copyButton = await screen.findByRole('button', { name: /copy setup command/i })
    // jsdom has no navigator.clipboard, which is exactly the non-secure-context
    // (plain HTTP) deployment case the message exists for.
    fireEvent.click(copyButton)

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(/select the setup command/i)
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/clipboard access/i)
  })

  test('applies a role template to simple chat agent instructions', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    const templateGroup = screen.getByRole('group', { name: /agent role templates/i })
    fireEvent.click(within(templateGroup).getByRole('button', { name: /review work/i }))

    expect(screen.getByLabelText(/^name$/i)).toHaveValue('Review Helper')
    expect((screen.getByLabelText(/agent instructions/i) as HTMLTextAreaElement).value).toContain(
      'confusing behavior'
    )
    expect(screen.queryByText(/^Reviewer$/)).toBeNull()
    expect(screen.queryByText(/prompt work/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      kind: 'provider',
      name: 'Review Helper',
      provider: 'anthropic',
      model: 'claude-sonnet-4-6',
      systemPrompt: expect.stringContaining('confusing behavior'),
    })
  })
})
