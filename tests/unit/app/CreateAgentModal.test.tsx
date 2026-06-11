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
      name: 'Default Work Lane',
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
    name: 'Default Work Lane',
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
    expect(screen.getByText('Name sets up this agent')).toBeInTheDocument()
    expect(screen.getByText(/claude in a managed workspace/i)).toBeInTheDocument()
    expect(screen.getByText('Project files included')).toBeInTheDocument()
    expect(screen.getByText(/workspace must be ready/i)).toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: /^work tool$/i })).toBeInTheDocument()
    expect(screen.getByLabelText(/project folder/i)).toBeInTheDocument()
    expect(screen.getByText(/primary project sets the default task context/i)).toBeInTheDocument()
    expect(screen.getAllByText(/primary project/i).length).toBeGreaterThan(0)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/choose a project first/i)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(
      /select a project in the sidebar/i
    )
    expect(screen.queryByLabelText(/^provider$/i)).toBeNull()
    expect(screen.queryByLabelText(/^model$/i)).toBeNull()
    expect(screen.queryByText(/Name seeds CLI agents/i)).toBeNull()
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

    expect(screen.getByText('Platform')).toBeInTheDocument()
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/project ready/i)
    expect(screen.getByText(/tasks default to this project/i)).toBeInTheDocument()
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

  test('creates and selects a work lane for the selected project', async () => {
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
    fireEvent.click(await screen.findByRole('button', { name: /create task group/i }))

    await waitFor(() =>
      expect(agentGroupApi.createGroup).toHaveBeenCalledWith({
        projectId: 'p1',
        name: 'Default Work Lane',
        description: 'This work lane lets agents receive board tasks.',
      })
    )
    expect(screen.getByRole('combobox', { name: /task group/i })).toHaveValue('group-new')
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/project ready/i)

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'CLI Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      workspaceId: 'w1',
      projectId: 'p1',
      groupId: 'group-new',
    })
  })

  test('switching to chat-only AI service hides work tool fields and shows Provider/Model', () => {
    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /chat-only ai service/i }))

    expect(screen.getByText('Instructions ready')).toBeInTheDocument()
    expect(screen.getByText(/anthropic chat-only agent/i)).toBeInTheDocument()
    expect(screen.getByText(/does not open project files/i)).toBeInTheDocument()
    expect(screen.getByText(/ai service must be ready/i)).toBeInTheDocument()
    expect(screen.queryByRole('combobox', { name: /^work tool$/i })).toBeNull()
    expect(screen.queryByLabelText(/project folder/i)).toBeNull()
    expect(screen.getByLabelText(/^provider$/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/^model$/i)).toBeInTheDocument()
  })

  test('updates runtime fit when the operator changes runtime choices', async () => {
    render(<CreateAgentModal />)

    fireEvent.change(screen.getByRole('combobox', { name: /^work tool$/i }), {
      target: { value: 'codex' },
    })
    expect(screen.getByText(/codex in a managed workspace/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    expect(screen.getByText(/codex on this computer/i)).toBeInTheDocument()
    expect(screen.getByText('Run the setup command')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: /chat-only ai service/i }))
    fireEvent.change(screen.getByLabelText(/^provider$/i), { target: { value: 'google' } })

    await waitFor(() => {
      expect(screen.getByText(/google chat-only agent/i)).toBeInTheDocument()
    })
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
    expect(screen.getByText(/where the work tool is installed/i)).toBeInTheDocument()
    expect(screen.queryByText(/where the CLI is installed/i)).toBeNull()
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
    expect(screen.getByRole('group', { name: /computer type/i })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /windows/i }))
    expect(oneLiner).toHaveValue(joinCommandPowershell)

    // Manual env block stays available behind the advanced section.
    expect(screen.getByText(/manual connection setup/i)).toBeInTheDocument()
    const manualHelp = screen.getByText(/connection helper/i)
    expect(manualHelp).toBeInTheDocument()
    expect(manualHelp.textContent).not.toMatch(/sidecar/i)
    expect(screen.getByLabelText(/manual setup environment/i)).toHaveValue(
      "export AGENT_ID='a-local'\nagentforge-sidecar"
    )
  })

  test('defaults to chat-only AI service when a verified provider exists', async () => {
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

    expect(screen.getByRole('radio', { name: /chat-only ai service/i })).toBeChecked()
    expect(screen.queryByRole('combobox', { name: /^work tool$/i })).toBeNull()
    expect(screen.getByLabelText(/^provider$/i)).toHaveValue('openai')
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

    fireEvent.click(screen.getByRole('radio', { name: /chat-only ai service/i }))
    fireEvent.change(screen.getByLabelText(/^provider$/i), { target: { value: 'openai' } })

    await waitFor(() => {
      expect(screen.getByLabelText(/^model$/i)).toHaveValue('gpt-4o')
    })
  })

  test('lists China-region providers and seeds the Zhipu GLM default model', async () => {
    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /chat-only ai service/i }))

    const providerSelect = screen.getByLabelText(/^provider$/i)
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
      expect(screen.getByLabelText(/^model$/i)).toHaveValue('glm-4.7')
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
    fireEvent.click(screen.getByRole('radio', { name: /chat-only ai service/i }))
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

  test('applies a role template to a provider agent prompt', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /chat-only ai service/i }))
    const templateGroup = screen.getByRole('group', { name: /agent role templates/i })
    fireEvent.click(within(templateGroup).getByRole('button', { name: /reviewer/i }))

    expect(screen.getByLabelText(/^name$/i)).toHaveValue('Review Agent')
    expect((screen.getByLabelText(/system prompt/i) as HTMLTextAreaElement).value).toContain(
      'security issues'
    )

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
