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
  test('renders Container CLI fields by default', () => {
    render(<CreateAgentModal />)

    expect(screen.getByRole('radio', { name: /container cli/i })).toBeChecked()
    expect(screen.getByTestId('agent-runtime-fit')).toBeInTheDocument()
    expect(screen.getByText(/claude container worker/i)).toBeInTheDocument()
    expect(screen.getByText('/workspace mounted')).toBeInTheDocument()
    expect(screen.getByText(/runtime container must start/i)).toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: /container cli/i })).toBeInTheDocument()
    expect(screen.getByLabelText(/working directory/i)).toBeInTheDocument()
    expect(screen.getByText(/shared workspace mount/i)).toBeInTheDocument()
    expect(screen.getAllByText(/primary project/i).length).toBeGreaterThan(0)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/choose a project first/i)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(
      /select a project in the sidebar/i
    )
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

  test('switching to Provider+Prompt hides CLI fields and shows Provider/Model', () => {
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-anthropic',
          provider: 'anthropic',
          displayName: 'Anthropic',
          model: 'claude-sonnet-4-6',
          priority: 1,
          isEnabled: true,
          isDefault: false,
          lastTestStatus: 'passed',
        },
      ],
    })

    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))

    expect(screen.getByText(/anthropic prompt worker/i)).toBeInTheDocument()
    expect(screen.getByText(/no direct workspace mount/i)).toBeInTheDocument()
    expect(screen.getByText(/provider key must be ready/i)).toBeInTheDocument()
    expect(screen.queryByRole('combobox', { name: /container cli/i })).toBeNull()
    expect(screen.queryByLabelText(/working directory/i)).toBeNull()
    expect(screen.getByLabelText(/^provider$/i)).toBeInTheDocument()
    // Model is derived from the configured provider (read-only).
    expect(screen.getByLabelText(/^model$/i)).toHaveValue('claude-sonnet-4-6')
  })

  test('Provider+Prompt with no configured provider shows a Settings hint and blocks submit', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))

    expect(screen.getByTestId('provider-empty-hint')).toHaveTextContent(
      /settings .* llm providers/i
    )
    expect(screen.queryByLabelText(/^provider$/i)).toBeNull()

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(/add and test a provider in settings/i)
    )
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('updates runtime fit when the operator changes runtime choices', async () => {
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-anthropic',
          provider: 'anthropic',
          displayName: 'Anthropic',
          model: 'claude-sonnet-4-6',
          priority: 1,
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        },
        {
          id: 'provider-google',
          provider: 'google',
          displayName: 'Google',
          model: 'gemini-2.5-pro',
          priority: 2,
          isEnabled: true,
          isDefault: false,
          lastTestStatus: 'passed',
        },
      ],
    })

    render(<CreateAgentModal />)

    // A verified provider exists, so the modal opens on Provider + Prompt.
    fireEvent.click(screen.getByRole('radio', { name: /local cli/i }))
    fireEvent.change(screen.getByRole('combobox', { name: /local cli/i }), {
      target: { value: 'codex' },
    })
    expect(screen.getByText(/codex local worker/i)).toBeInTheDocument()
    expect(screen.getByText('Run the join command')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: /container cli/i }))
    fireEvent.change(screen.getByRole('combobox', { name: /container cli/i }), {
      target: { value: 'codex' },
    })
    expect(screen.getByText(/codex container worker/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))
    fireEvent.change(screen.getByLabelText(/^provider$/i), { target: { value: 'provider-google' } })

    await waitFor(() => {
      expect(screen.getByText(/google prompt worker/i)).toBeInTheDocument()
    })
  })

  test('enrolls a Local CLI agent and shows the join command', async () => {
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

    fireEvent.click(screen.getByRole('radio', { name: /local cli/i }))
    fireEvent.change(screen.getByRole('combobox', { name: /local cli/i }), {
      target: { value: 'codex' },
    })
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Laptop Worker' } })
    fireEvent.change(screen.getByLabelText(/local working directory/i), {
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
  })

  test('shows the one-command join with an OS toggle when the server mints a join code', async () => {
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

    fireEvent.click(screen.getByRole('radio', { name: /local cli/i }))
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Laptop Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    // One-command join leads; the pasted command tracks the OS toggle.
    const oneLiner = await screen.findByLabelText(/one-command join/i)
    expect(oneLiner).toHaveValue(joinCommand)
    fireEvent.click(screen.getByRole('button', { name: /windows/i }))
    expect(oneLiner).toHaveValue(joinCommandPowershell)

    // Manual env block stays available behind the advanced section.
    expect(screen.getByText(/manual setup \(advanced\)/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/manual setup environment/i)).toHaveValue(
      "export AGENT_ID='a-local'\nagentforge-sidecar"
    )
  })

  test('defaults to Provider+Prompt when a verified provider exists', async () => {
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

    expect(screen.getByRole('radio', { name: /provider \+ prompt/i })).toBeChecked()
    expect(screen.queryByRole('combobox', { name: /container cli/i })).toBeNull()
    // The select chooses the configured provider by id; model is read-only.
    expect(screen.getByLabelText(/^provider$/i)).toHaveValue('provider-1')
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

  test('selecting a configured provider seeds its model from the gateway', async () => {
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-anthropic',
          provider: 'anthropic',
          displayName: 'Anthropic',
          model: 'claude-sonnet-4-6',
          priority: 1,
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        },
        {
          id: 'provider-openai',
          provider: 'openai',
          displayName: 'OpenAI Prod',
          model: 'gpt-5.4',
          priority: 2,
          isEnabled: true,
          isDefault: false,
          lastTestStatus: 'passed',
        },
      ],
    })

    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))
    fireEvent.change(screen.getByLabelText(/^provider$/i), { target: { value: 'provider-openai' } })

    await waitFor(() => {
      expect(screen.getByLabelText(/^model$/i)).toHaveValue('gpt-5.4')
    })
  })

  test('lists configured providers (including China-region) by display name and model', async () => {
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-anthropic',
          provider: 'anthropic',
          displayName: 'Anthropic',
          model: 'claude-sonnet-4-6',
          priority: 1,
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        },
        {
          id: 'provider-zhipu',
          provider: 'zhipu',
          displayName: 'Zhipu GLM',
          model: 'glm-4.7',
          priority: 2,
          isEnabled: true,
          isDefault: false,
          lastTestStatus: 'passed',
        },
      ],
    })

    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))

    const providerSelect = screen.getByLabelText(/^provider$/i)
    // Each option shows the display name and the model.
    expect(
      within(providerSelect).getByRole('option', { name: /zhipu glm · glm-4\.7/i })
    ).toBeInTheDocument()
    expect(
      within(providerSelect).getByRole('option', { name: /anthropic · claude-sonnet-4-6/i })
    ).toBeInTheDocument()

    fireEvent.change(providerSelect, { target: { value: 'provider-zhipu' } })

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
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-anthropic',
          provider: 'anthropic',
          displayName: 'Anthropic',
          model: 'claude-sonnet-4-6',
          priority: 1,
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        },
      ],
    })

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))
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

  test('empty name shows a visible error instead of silently ignoring the click', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Name is required'))
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('whitespace-only name shows the same error', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: '   ' } })
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Name is required'))
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('provider kind whose configured provider has no model shows a visible error', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)
    // A configured provider with a blank model is the only "missing model" path
    // now that the model field is derived (read-only) from the gateway.
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-broken',
          provider: 'anthropic',
          displayName: 'Anthropic',
          model: '',
          priority: 1,
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        },
      ],
    })

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Provider and model are required')
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
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Name is required'))
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
    fireEvent.click(screen.getByRole('radio', { name: /local cli/i }))
    fireEvent.click(screen.getByRole('button', { name: /^create agent$/i }))

    const copyButton = await screen.findByRole('button', { name: /copy command/i })
    // jsdom has no navigator.clipboard, which is exactly the non-secure-context
    // (plain HTTP) deployment case the message exists for.
    fireEvent.click(copyButton)

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(/copy is unavailable here/i)
    )
  })

  test('applies a role template to a provider agent prompt', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-anthropic',
          provider: 'anthropic',
          displayName: 'Anthropic',
          model: 'claude-sonnet-4-6',
          priority: 1,
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'passed',
        },
      ],
    })

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))
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
