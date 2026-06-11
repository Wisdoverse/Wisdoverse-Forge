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
  useNavigationStore.setState({ selectedProjectId: null, projects: {} })
})

describe('CreateAgentModal', () => {
  test('renders managed workspace fields by default', () => {
    render(<CreateAgentModal />)

    expect(screen.getByRole('radio', { name: /managed workspace/i })).toBeChecked()
    expect(screen.getByTestId('agent-runtime-fit')).toBeInTheDocument()
    expect(screen.getByText(/claude in a managed workspace/i)).toBeInTheDocument()
    expect(
      screen.getByText(/project files and a ready place to check, change, and verify them/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Project files available')).toBeInTheDocument()
    expect(screen.getByText(/workspace must be online/i)).toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: /managed work tool/i })).toBeInTheDocument()
    expect(screen.getByLabelText(/project files folder/i)).toBeInTheDocument()
    expect(screen.getByText(/managed workspace can include several projects/i)).toBeInTheDocument()
    expect(screen.getAllByText(/starting project/i).length).toBeGreaterThan(0)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(
      /choose a starting project first/i
    )
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(
      /select a project in the sidebar/i
    )
    expect(screen.queryByText(/primary project/i)).toBeNull()
    expect(screen.queryByText(/default work area/i)).toBeNull()
    expect(screen.queryByLabelText(/^ai service$/i)).toBeNull()
    expect(screen.queryByLabelText(/^model$/i)).toBeNull()
  })

  test('shows selected project as the starting project context', () => {
    useNavigationStore.setState({
      selectedProjectId: 'p1',
      agentGroups: [],
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

    expect(screen.getByText('Platform')).toBeInTheDocument()
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/project ready/i)
    expect(screen.getByText(/new tasks start in this project/i)).toBeInTheDocument()
  })

  test('submits selected project workspace context', async () => {
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
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'File Work Agent' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      workspaceId: 'w1',
      projectId: 'p1',
    })
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
    fireEvent.click(await screen.findByRole('button', { name: /create task queue/i }))

    await waitFor(() =>
      expect(agentGroupApi.createGroup).toHaveBeenCalledWith({
        projectId: 'p1',
        name: 'Default Task Queue',
        description: 'This task queue lets agents receive board tasks.',
      })
    )
    expect(screen.getByRole('combobox', { name: /task queue/i })).toHaveValue('group-new')
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/project ready/i)

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'File Work Agent' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      workspaceId: 'w1',
      projectId: 'p1',
      groupId: 'group-new',
    })
  })

  test('explains task queue creation permission failures without raw API text', async () => {
    vi.mocked(agentGroupApi.createGroup).mockRejectedValueOnce(new Error('HTTP 403: Forbidden'))
    useNavigationStore.setState({
      selectedProjectId: 'p1',
      agentGroups: [],
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

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Task queue was not created. Ask an owner or admin to let you create and manage task queues in this project.'
    )
    expect(screen.queryByText(/HTTP 403/i)).toBeNull()
  })

  test('switching to Chat-only agent hides work tool fields and shows AI service fields', () => {
    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /chat-only agent/i }))

    expect(screen.getByText(/anthropic chat-only agent/i)).toBeInTheDocument()
    expect(screen.getByText(/does not open project files/i)).toBeInTheDocument()
    expect(screen.getByText(/ai service checked/i)).toBeInTheDocument()
    expect(screen.queryByRole('combobox', { name: /managed work tool/i })).toBeNull()
    expect(
      screen.queryByLabelText(/project files folder|project folder on this computer/i)
    ).toBeNull()
    expect(screen.getByLabelText(/^ai service$/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/^model$/i)).toHaveAccessibleDescription(
      /keep the suggested model/i
    )
    expect(screen.queryByText(/command window/i)).toBeNull()
  })

  test('guides users to name an agent before creating it', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Name this agent before creating it. Example: Review Agent or File Work Agent.'
    )
    expect(screen.queryByText('Name is required')).toBeNull()
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('guides users to choose AI service fields before creating a chat-only agent', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Prompt Worker' } })
    fireEvent.click(screen.getByRole('radio', { name: /chat-only agent/i }))
    fireEvent.change(screen.getByLabelText(/^model$/i), { target: { value: '' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Choose an AI service and model before creating this chat-only agent.'
    )
    expect(screen.queryByText('Provider and model are required')).toBeNull()
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('updates work fit when the operator changes work type choices', async () => {
    render(<CreateAgentModal />)

    fireEvent.change(screen.getByRole('combobox', { name: /managed work tool/i }), {
      target: { value: 'codex' },
    })
    expect(screen.getByText(/codex in a managed workspace/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    expect(screen.getByText(/codex on this computer/i)).toBeInTheDocument()
    expect(screen.getByText(/forge should manage the agent from the platform/i)).toBeInTheDocument()
    expect(screen.getByText('Managed from Forge')).toBeInTheDocument()
    expect(screen.queryByText(/local C[L]I sessions/i)).toBeNull()
    expect(screen.getByText('Run the join command here')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: /chat-only agent/i }))
    fireEvent.change(screen.getByLabelText(/^ai service$/i), { target: { value: 'google' } })

    await waitFor(() => {
      expect(screen.getByText(/google chat-only agent/i)).toBeInTheDocument()
    })
  })

  test('enrolls an agent on this computer and shows the join command', async () => {
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
    fireEvent.change(screen.getByRole('combobox', { name: /local work tool/i }), {
      target: { value: 'codex' },
    })
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Laptop Worker' } })
    fireEvent.change(screen.getByLabelText(/project folder on this computer/i), {
      target: { value: '/Users/me/project' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(enrollLocalAgent).toHaveBeenCalledTimes(1))
    expect(enrollLocalAgent.mock.calls[0][0]).toMatchObject({
      name: 'Laptop Worker',
      cliTool: 'codex',
      cwd: '/Users/me/project',
    })
    expect(await screen.findByLabelText(/join command/i)).toHaveValue(
      "export AGENT_ID='a-local'\nagentforge-sidecar"
    )
    expect(screen.getByText('What to do next')).toBeInTheDocument()
    expect(screen.getByText(/forge created the managed agent first/i)).toBeInTheDocument()
    expect(screen.getByText(/connect this machine to that agent/i)).toBeInTheDocument()
    expect(screen.getByText(/project folder on this computer/i)).toBeInTheDocument()
    expect(screen.getByText(/window where you run commands/i)).toBeInTheDocument()
    expect(screen.getByText(/keep that window open/i)).toBeInTheDocument()
    expect(screen.getByText(/run the same command again to reconnect/i)).toBeInTheDocument()
    expect(screen.queryByText(/command window/i)).toBeNull()
  })

  test('defaults to chat-only agent when a verified AI service exists', async () => {
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

    expect(screen.getByRole('radio', { name: /chat-only agent/i })).toBeChecked()
    expect(screen.queryByRole('combobox', { name: /managed work tool/i })).toBeNull()
    expect(screen.getByLabelText(/^ai service$/i)).toHaveValue('openai')
    expect(screen.getByLabelText(/^model$/i)).toHaveValue('gpt-5.5')

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

    fireEvent.click(screen.getByRole('radio', { name: /chat-only agent/i }))
    fireEvent.change(screen.getByLabelText(/^ai service$/i), { target: { value: 'openai' } })

    await waitFor(() => {
      expect(screen.getByLabelText(/^model$/i)).toHaveValue('gpt-4o')
    })
  })

  test('submits cli kind without provider/model fields', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'File Work Agent' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    const payload = createAgent.mock.calls[0][0]
    expect(payload).toMatchObject({
      kind: 'cli',
      name: 'File Work Agent',
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
    fireEvent.click(screen.getByRole('radio', { name: /chat-only agent/i }))
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

  test('applies a role template to a chat-only agent prompt', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /chat-only agent/i }))
    const templateGroup = screen.getByRole('group', { name: /agent role templates/i })
    fireEvent.click(within(templateGroup).getByRole('button', { name: /reviewer/i }))

    expect(screen.getByLabelText(/^name$/i)).toHaveValue('Review Agent')
    expect(
      (screen.getByLabelText(/instructions for this agent/i) as HTMLTextAreaElement).value
    ).toContain('security issues')
    expect(screen.getByText(/tell the agent how to behave every time/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      kind: 'provider',
      name: 'Review Agent',
      provider: 'anthropic',
      model: 'claude-sonnet-4-6',
      systemPrompt: expect.stringContaining('security issues'),
    })
  })
})
