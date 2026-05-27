import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { CreateAgentModal } from '@app/features/agents/CreateAgentModal'
import { useAgentsStore } from '@app/shared/model/agents.store'
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
    fireEvent.click(await screen.findByRole('button', { name: /create default work lane/i }))

    await waitFor(() =>
      expect(agentGroupApi.createGroup).toHaveBeenCalledWith({
        projectId: 'p1',
        name: 'Default Work Lane',
        description: 'This work lane lets agents receive board tasks.',
      })
    )
    expect(screen.getByRole('combobox', { name: /work lane/i })).toHaveValue('group-new')
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/default work lane/i)

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
    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))

    expect(screen.getByText(/anthropic prompt worker/i)).toBeInTheDocument()
    expect(screen.getByText(/no direct workspace mount/i)).toBeInTheDocument()
    expect(screen.getByText(/provider key must be ready/i)).toBeInTheDocument()
    expect(screen.queryByRole('combobox', { name: /container cli/i })).toBeNull()
    expect(screen.queryByLabelText(/working directory/i)).toBeNull()
    expect(screen.getByLabelText(/^provider$/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/^model$/i)).toBeInTheDocument()
  })

  test('updates runtime fit when the operator changes runtime choices', async () => {
    render(<CreateAgentModal />)

    fireEvent.change(screen.getByRole('combobox', { name: /container cli/i }), {
      target: { value: 'codex' },
    })
    expect(screen.getByText(/codex container worker/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))
    fireEvent.change(screen.getByLabelText(/^provider$/i), { target: { value: 'google' } })

    await waitFor(() => {
      expect(screen.getByText(/google prompt worker/i)).toBeInTheDocument()
    })
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

    fireEvent.click(screen.getByRole('radio', { name: /provider \+ prompt/i }))
    fireEvent.change(screen.getByLabelText(/^provider$/i), { target: { value: 'openai' } })

    await waitFor(() => {
      expect(screen.getByLabelText(/^model$/i)).toHaveValue('gpt-4o')
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

  test('applies a role template to a provider agent prompt', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

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
