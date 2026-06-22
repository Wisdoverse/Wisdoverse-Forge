import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { CreateAgentModal } from '@app/features/agents/CreateAgentModal'
import { useAgentsStore } from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { agentGroupApi } from '@app/entities/agent-group'

vi.mock('@app/entities/agent-group', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/entities/agent-group')>()

  return {
    ...actual,
    agentGroupApi: {
      getGroups: vi.fn().mockResolvedValue([]),
      createGroup: vi.fn().mockResolvedValue({
        id: 'group-new',
        name: 'Default Waiting Place',
        projectId: 'p1',
      }),
    },
  }
})

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
    name: 'Default Waiting Place',
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

  function selectProject() {
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
  }

  test('renders project-file fields by default', () => {
    render(<CreateAgentModal />)

    expect(screen.getByRole('heading', { name: 'New agent' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: /project files/i })).toBeChecked()
    expect(screen.getByTestId('agent-runtime-fit')).toBeInTheDocument()
    expect(screen.getByText('Pick a starter template')).toBeInTheDocument()
    expect(screen.queryByText('Start with a role')).toBeNull()
    expect(screen.getByText('Where should this agent work?')).toBeInTheDocument()
    expect(screen.getByRole('radiogroup', { name: /where should this agent work/i })).toBeDefined()
    expect(
      screen.getByText(/start with what this agent should be allowed to touch/i)
    ).toBeInTheDocument()
    expect(
      screen.getByText(
        /Use this for the usual setup when the agent should edit shared project files/i
      )
    ).toBeInTheDocument()
    expect(screen.getByText('Most file work')).toBeInTheDocument()
    expect(
      screen.getByText(/Use this when files or tools must stay on this computer/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Local files')).toBeInTheDocument()
    expect(
      screen.getByText(
        /Use this for questions, writing, and result checks that do not need file edits/i
      )
    ).toBeInTheDocument()
    expect(screen.getByText('No files')).toBeInTheDocument()
    expect(screen.queryByText('Choose work style')).toBeNull()
    expect(screen.getByText('Fills in the name and first task')).toBeInTheDocument()
    expect(screen.getByText('Updates the work and checks it')).toBeInTheDocument()
    expect(screen.queryByText('Builds changes and checks them')).toBeNull()
    expect(screen.getAllByText(/claude with project files/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Project files are included')).toBeInTheDocument()
    expect(screen.getAllByText('Where it works').length).toBeGreaterThan(0)
    expect(screen.getByText('Shared project folder')).toBeInTheDocument()
    expect(screen.getByText('Check Where agents work in Settings')).toBeInTheDocument()
    expect(screen.getByText('Can edit files')).toBeInTheDocument()
    expect(
      screen.getByText(
        /not sure\? use project files when the agent should edit shared project files, this computer when files must stay local, or simple chat agent after an AI service is ready/i
      )
    ).toBeInTheDocument()
    expect(screen.queryByText(/ready workspace managed by forge/i)).toBeNull()
    expect(screen.queryByText(/Forge project area/i)).toBeNull()
    expect(screen.queryByText(/agent location/i)).toBeNull()
    expect(screen.queryByText(/workspace must be ready/i)).toBeNull()
    expect(screen.queryByText(/choose a runtime/i)).toBeNull()
    expect(screen.queryByText('File work')).toBeNull()
    expect(screen.getByRole('combobox', { name: /^work tool$/i })).toBeInTheDocument()
    expect(screen.getByLabelText(/work folder/i)).toBeInTheDocument()
    expect(screen.getByText(/keep the suggested folder/i)).toBeInTheDocument()
    expect(screen.getByText(/new tasks start from the project shown above/i)).toBeInTheDocument()
    expect(screen.queryByText(/use \/workspace unless/i)).toBeNull()
    expect(screen.queryByText(/default task context/i)).toBeNull()
    expect(screen.getAllByText(/project for new tasks/i).length).toBeGreaterThan(0)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/open project settings/i)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(
      /open project settings to create or choose a project before creating this file-working agent/i
    )
    expect(screen.getByTestId('agent-work-readiness')).not.toHaveTextContent(
      /agent can still be created first/i
    )
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/choose a project later/i)
    expect(screen.getByTestId('agent-work-readiness')).not.toHaveTextContent(/no primary project/i)
    expect(screen.queryByText(/primary project/i)).toBeNull()
    expect(screen.getByTestId('agent-work-readiness')).not.toHaveTextContent(
      /select a project in the sidebar/i
    )
    expect(screen.queryByRole('button', { name: /open project settings/i })).toBeNull()
    const review = screen.getByTestId('agent-create-review')
    expect(within(review).getByText('Before you create')).toBeInTheDocument()
    expect(within(review).getByText('Where it works')).toBeInTheDocument()
    expect(within(review).getByText('After creation')).toBeInTheDocument()
    expect(within(review).queryByText('Work style')).toBeNull()
    expect(within(review).queryByText('Created state')).toBeNull()
    expect(within(review).getByText(/claude with project files/i)).toBeInTheDocument()
    expect(within(review).getByText('Choose a project before sending tasks.')).toBeInTheDocument()
    expect(within(review).queryByText('No project selected yet')).toBeNull()
    expect(
      within(review).getByText('Choose a project later before sending tasks.')
    ).toBeInTheDocument()
    expect(
      within(review).getByText('Wait until it shows Ready, then send one small task from Tasks.')
    ).toBeInTheDocument()
    expect(
      within(review).queryByText('Start the agent, then send one small task from Tasks.')
    ).toBeNull()
    expect(
      within(review).getByText('Forge starts it after the project file area is ready.')
    ).toBeInTheDocument()
    expect(screen.queryByLabelText(/^ai service$/i)).toBeNull()
    expect(screen.queryByLabelText(/^ai model$/i)).toBeNull()
    expect(screen.queryByText(/Name seeds CLI agents/i)).toBeNull()
    expect(screen.queryByLabelText(/^provider$/i)).toBeNull()
    expect(screen.queryByLabelText(/^model$/i)).toBeNull()
  })

  test('closes the modal and opens project settings when no primary project is selected', () => {
    const onOpenProjectsSetup = vi.fn()

    render(<CreateAgentModal onOpenProjectsSetup={onOpenProjectsSetup} />)

    const readiness = screen.getByTestId('agent-work-readiness')
    fireEvent.click(within(readiness).getByRole('button', { name: /open project settings/i }))

    expect(onOpenProjectsSetup).toHaveBeenCalledTimes(1)
    expect(useAgentsStore.getState().createModalOpen).toBe(false)
    expect(screen.queryByRole('dialog', { name: /new agent/i })).toBeNull()
  })

  test('uses the primary button to open project settings before file-working agent creation', () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    const onOpenProjectsSetup = vi.fn()
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal onOpenProjectsSetup={onOpenProjectsSetup} />)

    expect(screen.queryByRole('button', { name: /^add agent$/i })).toBeNull()
    fireEvent.click(screen.getAllByRole('button', { name: /^open project settings$/i }).at(-1)!)

    expect(onOpenProjectsSetup).toHaveBeenCalledTimes(1)
    expect(useAgentsStore.getState().createModalOpen).toBe(false)
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('blocks project-file agent creation until a project is selected without a route callback', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'CLI Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(/open project settings, create or choose a project/i)
    expect(alert).toHaveTextContent(/agents that work with files need a project first/i)
    expect(createAgent).not.toHaveBeenCalled()
    expect(within(alert).queryByRole('button', { name: /open project settings/i })).toBeNull()
  })

  test('shows selected project as the project for new tasks', () => {
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
        'Set up where tasks wait here when you want new tasks to wait in one place.'
      )
    ).toBeInTheDocument()
  })

  test('guides users when waiting places exist but none is selected yet', () => {
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
      agentGroups: [{ id: 'group-1', name: 'Review Queue', projectId: 'p1' }],
    })

    render(<CreateAgentModal />)

    const review = screen.getByTestId('agent-create-review')
    expect(
      within(review).getByText('Choose where tasks wait now, or set it later from Tasks.')
    ).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Review waiting place' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: /set this later/i })).toBeInTheDocument()
    expect(within(review).getByText('Where tasks wait')).toBeInTheDocument()
    expect(within(review).queryByText('No task queue selected yet')).toBeNull()
    expect(screen.queryByRole('option', { name: /^no task queue$/i })).toBeNull()
    expect(screen.queryByRole('option', { name: 'Review Queue' })).toBeNull()
    expect(screen.queryByText('Task queue')).toBeNull()

    fireEvent.change(screen.getByLabelText(/where tasks wait/i), {
      target: { value: 'group-1' },
    })

    expect(within(review).getByText('Review waiting place')).toBeInTheDocument()
    expect(within(review).queryByText('Review Queue')).toBeNull()
  })

  test('submits the selected project as the execution boundary', async () => {
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
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

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
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Name this agent before creating it.'
    )
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('sets up and selects where tasks wait for the selected project', async () => {
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
      screen.getByText(/starter place for this project so new tasks have somewhere clear to wait/i)
    ).toBeInTheDocument()
    fireEvent.click(await screen.findByRole('button', { name: /set up where tasks wait/i }))

    await waitFor(() =>
      expect(agentGroupApi.createGroup).toHaveBeenCalledWith({
        projectId: 'p1',
        name: 'Default Waiting Place',
        description:
          'Starter place for this project. New tasks wait here until an agent can take them.',
      })
    )
    expect(screen.getByRole('combobox', { name: /where tasks wait/i })).toHaveValue('group-new')
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/project ready/i)
    expect(
      screen.getByText(/new tasks wait here until an available agent can take them/i)
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('agent-create-review')).getByText('Default Waiting Place')
    ).toBeInTheDocument()
    expect(screen.queryByText('Task queue')).toBeNull()
    expect(screen.queryByText(/work a place to wait until this agent can take it/i)).toBeNull()
    expect(screen.queryByText(new RegExp('board\\s+tasks', 'i'))).toBeNull()

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'CLI Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      workspaceId: 'w1',
      projectId: 'p1',
      groupId: 'group-new',
    })
  })

  test('hides raw waiting-place creation errors while setting up the default place', async () => {
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
    fireEvent.click(await screen.findByRole('button', { name: /set up where tasks wait/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Wait a few minutes, then set up where tasks wait again.')
    expect(alert).toHaveTextContent(
      'ask an owner or admin to check where tasks wait in this project'
    )
    expect(alert).not.toHaveTextContent('task routing setup')
    expect(alert).not.toHaveTextContent('task queue')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('database unavailable')
  })

  test('switching to simple chat agent hides work tool fields and shows AI service details', () => {
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

    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))

    expect(screen.getByText('Fills in the name and what this agent should do')).toBeInTheDocument()
    expect(
      screen.getAllByText(/anthropic for questions and result checks/i).length
    ).toBeGreaterThan(0)
    expect(screen.getByText(/questions, writing, and checking results/i)).toBeInTheDocument()
    expect(screen.getByText(/does not open project files/i)).toBeInTheDocument()
    expect(screen.getByText('Check AI service in Settings')).toBeInTheDocument()
    expect(screen.queryByText(/ai service must be checked/i)).toBeNull()
    expect(screen.queryByRole('combobox', { name: /^work tool$/i })).toBeNull()
    expect(screen.queryByLabelText(/work folder/i)).toBeNull()
    expect(screen.getByLabelText(/^ai service$/i)).toHaveValue('provider-anthropic')
    expect(screen.getByLabelText(/^saved ai service setup$/i)).toHaveValue('claude-sonnet-4-6')
    expect(screen.getByLabelText(/^saved ai service setup$/i)).toHaveAttribute(
      'placeholder',
      'Filled from AI service settings'
    )
    expect(screen.queryByLabelText(/^ai model$/i)).toBeNull()
    expect(screen.getByText(/choose the AI service name you set up/i)).toBeInTheDocument()
    expect(screen.getByText(/comes from the checked AI service in Settings/i)).toBeInTheDocument()
    expect(screen.getByText(/you do not need to change it here/i)).toBeInTheDocument()
    expect(screen.queryByText(/AI services settings/i)).toBeNull()
    const review = screen.getByTestId('agent-create-review')
    expect(
      within(review).getByText(
        'Ask a first question, or send a task to check a result that does not need files.'
      )
    ).toBeInTheDocument()
    expect(
      within(review).getByText(
        'Ready for questions and result checks after the AI service is connected.'
      )
    ).toBeInTheDocument()
    expect(screen.queryByLabelText(/^model name$/i)).toBeNull()
    expect(screen.queryByLabelText(/^provider$/i)).toBeNull()
    expect(screen.queryByLabelText(/^model$/i)).toBeNull()
  })

  test('simple chat agent with no checked AI service shows a Settings hint and blocks submit', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))

    const providerHint = screen.getByTestId('provider-empty-hint')
    expect(providerHint).toHaveTextContent(/add and check an ai service first/i)
    expect(providerHint).toHaveTextContent(/open ai service settings/i)
    expect(providerHint).toHaveTextContent(/paste the key from that service/i)
    expect(providerHint).toHaveTextContent(/choose check connection/i)
    expect(providerHint).toHaveTextContent(/service shows ready/i)
    expect(providerHint).not.toHaveTextContent(/no ai service ready yet/i)
    expect(providerHint).not.toHaveTextContent(/service access key/i)
    expect(providerHint).not.toHaveTextContent(/settings > ai services/i)
    expect(providerHint).not.toHaveTextContent(/ai services settings/i)
    expect(providerHint).not.toHaveTextContent(/click check/i)
    expect(screen.getByRole('link', { name: /open ai service settings/i })).toHaveAttribute(
      'href',
      '/settings/providers'
    )
    expect(screen.queryByLabelText(/^ai service$/i)).toBeNull()

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    let alert: HTMLElement | null = null
    await waitFor(() =>
      expect((alert = screen.getByRole('alert'))).toHaveTextContent(/open ai service settings/i)
    )
    expect(alert).toHaveTextContent(/paste the key from that service/i)
    expect(alert).toHaveTextContent(/come back when it shows ready/i)
    expect(alert).not.toHaveTextContent(/service access key/i)
    expect(alert).not.toHaveTextContent(/until it says ready/i)
    expect(alert).not.toHaveTextContent(/settings > ai services/i)
    expect(alert).not.toHaveTextContent(/ai services settings/i)
    expect(alert).not.toHaveTextContent(/click check/i)
    expect(
      within(alert as HTMLElement).getByRole('link', { name: /open ai service settings/i })
    ).toHaveAttribute('href', '/settings/providers')
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

    fireEvent.click(screen.getByRole('radio', { name: /project files/i }))
    fireEvent.change(screen.getByRole('combobox', { name: /^work tool$/i }), {
      target: { value: 'codex' },
    })
    expect(screen.getAllByText(/codex with project files/i).length).toBeGreaterThan(0)

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
    expect(screen.getByText('Paste setup text in Terminal or PowerShell')).toBeInTheDocument()
    expect(screen.getByText('Uses this computer')).toBeInTheDocument()
    const localNextStep = screen.getByTestId('local-agent-before-create')
    expect(localNextStep).toHaveTextContent('Before you create this computer agent')
    expect(localNextStep).toHaveTextContent(
      'Choose the folder this computer should work in. If you are not sure, leave it blank.'
    )
    expect(localNextStep).toHaveTextContent(
      'After you choose Add agent, copy the setup text and paste it into Terminal or PowerShell on this computer.'
    )
    expect(localNextStep).toHaveTextContent(
      'Success looks like this agent changing to Ready on the Agents page.'
    )
    expect(localNextStep).not.toHaveTextContent(/manual connection/i)
    expect(localNextStep).not.toHaveTextContent(/runtime/i)
    const localReview = screen.getByTestId('agent-create-review')
    expect(
      within(localReview).getByText(
        'Paste the setup text in Terminal or PowerShell and keep that window open.'
      )
    ).toBeInTheDocument()
    expect(screen.queryByText(/command app/i)).toBeNull()
    expect(
      within(localReview).getByText(
        'Forge creates the agent, then shows setup steps for this computer.'
      )
    ).toBeInTheDocument()
    expect(
      screen.queryByText(new RegExp(['work tool', 'installed', 'your computer'].join('.*'), 'i'))
    ).toBeNull()
    expect(screen.queryByText(new RegExp(['Local', 'work'].join('\\s+')))).toBeNull()

    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.change(screen.getByLabelText(/^ai service$/i), {
      target: { value: 'provider-google' },
    })

    await waitFor(() => {
      expect(screen.getAllByText(/google for questions and result checks/i).length).toBeGreaterThan(
        0
      )
    })
    expect(screen.getByText('AI service only')).toBeInTheDocument()
    expect(screen.getByText('Check AI service in Settings')).toBeInTheDocument()
    expect(
      screen.queryByText(new RegExp(['AI service', 'must be checked'].join('.*'), 'i'))
    ).toBeNull()
  })

  test('enrolls an agent on this computer and shows the setup text', async () => {
    selectProject()
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
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    await waitFor(() => expect(enrollLocalAgent).toHaveBeenCalledTimes(1))
    expect(enrollLocalAgent.mock.calls[0][0]).toMatchObject({
      name: 'Laptop Worker',
      cliTool: 'codex',
      cwd: '/Users/me/project',
      workspaceId: 'w1',
      projectId: 'p1',
    })
    expect(await screen.findByLabelText(/^setup text$/i)).toHaveValue(
      "export AGENT_ID='a-local'\nagentforge-sidecar"
    )
    expect(screen.queryByLabelText(/setup command/i)).toBeNull()
    expect(screen.getByText('Connect this computer')).toBeInTheDocument()
    expect(screen.getByText('This computer handles tasks')).toBeInTheDocument()
    expect(screen.queryByText(/agent managed by forge/i)).not.toBeInTheDocument()
    expect(screen.getByText(/Open Terminal or PowerShell on that computer/i)).toBeInTheDocument()
    expect(
      screen.getByText(/Forge will manage its tasks, status, and history/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/files stay on that computer/i)).toBeInTheDocument()
    expect(screen.getByText('1. Copy the setup text.')).toBeInTheDocument()
    expect(screen.getByText(/paste it into Terminal or PowerShell/i)).toBeInTheDocument()
    expect(
      screen.getByText(/changes from Not connected to Ready on the Agents page/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/Closing that window disconnects this agent/i)).toBeInTheDocument()
    expect(screen.getByText(/come back to Forge, open Agents/i)).toBeInTheDocument()
    expect(screen.queryByText(/command app/i)).toBeNull()
    expect(screen.getByRole('button', { name: /close and watch agents/i })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /^done$/i })).toBeNull()
    expect(screen.queryByText(previousCliInstallCopy)).toBeNull()
    expect(screen.queryByText(previousManualConnectionCopy)).toBeNull()
    expect(screen.queryByText(new RegExp(['local', 'agent', 'join'].join('.*'), 'i'))).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /close and watch agents/i }))
    expect(screen.queryByRole('dialog', { name: /connect this computer/i })).toBeNull()
  })

  test('shows the setup command with an OS toggle when the server mints a join code', async () => {
    selectProject()
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
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    // The setup text leads; the pasted text tracks the OS toggle.
    const oneLiner = await screen.findByLabelText(/^setup text$/i)
    expect(oneLiner).toHaveValue(joinCommand)
    expect(screen.getByText(/Forge will show it as an agent here/i)).toBeInTheDocument()
    expect(
      screen.getByText(/let you send tasks to it, and keep its status and history/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/Files stay on that computer/i)).toBeInTheDocument()
    expect(screen.getByTestId('local-agent-paste-hint')).toHaveTextContent(
      'Open Terminal on macOS/Linux, then paste this setup text.'
    )
    expect(
      screen.getByText(/choose Add another agent to get fresh setup text/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/fresh command/i)).toBeNull()
    expect(screen.getByText('1. Copy the setup text.')).toBeInTheDocument()
    expect(screen.getByText(/paste it into that window/i)).toBeInTheDocument()
    expect(
      screen.getByText(/changes from Not connected to Ready on the Agents page/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/Closing that window disconnects this agent/i)).toBeInTheDocument()
    expect(screen.getByText(/come back to Forge, open Agents/i)).toBeInTheDocument()
    expect(screen.queryByText(/shows online/i)).toBeNull()
    expect(screen.queryByText(/agent fleet/i)).toBeNull()
    expect(screen.getByRole('group', { name: /computer type/i })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /windows/i }))
    expect(oneLiner).toHaveValue(joinCommandPowershell)
    expect(screen.getByTestId('local-agent-paste-hint')).toHaveTextContent(
      'Open PowerShell on Windows, then paste this setup text.'
    )

    // Backup setup text stays available without exposing advanced connection jargon.
    expect(screen.getByText(/if the setup text does not work/i)).toBeInTheDocument()
    const backupHelp = screen.getByText(/backup setup text/i)
    expect(backupHelp).toBeInTheDocument()
    expect(backupHelp.textContent).toMatch(/same window/)
    expect(backupHelp.textContent).not.toMatch(/same command app/)
    expect(backupHelp.textContent).not.toMatch(/advanced/i)
    expect(backupHelp.textContent).not.toMatch(/sidecar/i)
    expect(screen.queryByText(/backup setup values/i)).toBeNull()
    expect(screen.queryByText(previousManualConnectionCopy)).toBeNull()
    expect(screen.getByRole('button', { name: /copy backup setup/i })).toBeInTheDocument()
    expect(screen.getByLabelText(/backup setup text/i)).toHaveValue(
      "export AGENT_ID='a-local'\nagentforge-sidecar"
    )
  })

  test('guides Windows users to backup setup text when the one-line command is missing', async () => {
    selectProject()
    const joinCommand =
      'curl -fsSL https://forge.example.com/api/v1/agents/local-join/script | sh -s -- --code afj_test'
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
        shellExports: "$env:AGENT_ID = 'a-local'\nagentforge-sidecar",
        sidecarCommand: 'agentforge-sidecar',
        serverUrl: 'https://forge.example.com',
        joinCode: 'afj_test',
        joinCommand,
        joinCommandPowershell: null,
      },
    })
    useAgentsStore.setState({ enrollLocalAgent } as never)

    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Laptop Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    expect(await screen.findByLabelText(/^setup text$/i)).toHaveValue(joinCommand)
    fireEvent.click(screen.getByRole('button', { name: /windows/i }))

    expect(screen.queryByLabelText(/setup command/i)).toBeNull()
    expect(screen.getByText(/Windows setup needs backup setup text/i)).toBeInTheDocument()
    expect(screen.queryByText(/one-line Windows setup text is not ready/i)).toBeNull()
    expect(screen.getByTestId('local-agent-paste-hint')).toHaveTextContent(
      'Use the backup setup text below for Windows.'
    )
    expect(screen.queryByText(/backup setup values/i)).toBeNull()
    expect(screen.getByRole('button', { name: /use backup setup text/i })).toBeDisabled()
    expect(screen.getByLabelText(/backup setup text/i)).toHaveValue(
      "$env:AGENT_ID = 'a-local'\nagentforge-sidecar"
    )
  })

  test('uses a beginner fallback name when the setup response has no agent name', async () => {
    selectProject()
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
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    expect(await screen.findByText('This computer agent')).toBeInTheDocument()
    expect(screen.getByText('This computer handles tasks')).toBeInTheDocument()
    expect(screen.queryByText('Agent managed by Forge')).not.toBeInTheDocument()
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
    expect(screen.getByLabelText(/^ai service$/i)).toHaveValue('provider-1')
    expect(screen.getByLabelText(/^saved ai service setup$/i)).toHaveValue('gpt-5.5')

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

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

    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.change(screen.getByLabelText(/^ai service$/i), {
      target: { value: 'provider-openai' },
    })

    await waitFor(() => {
      expect(screen.getByLabelText(/^saved ai service setup$/i)).toHaveValue('gpt-5.4')
    })
  })

  test('lists configured providers (including China-region) by display name and saved setup', async () => {
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

    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))

    const providerSelect = screen.getByLabelText(/^ai service$/i)
    // Each option shows the display name and says the setup is already saved.
    expect(
      within(providerSelect).getByRole('option', { name: /zhipu glm · saved setup/i })
    ).toBeInTheDocument()
    expect(
      within(providerSelect).getByRole('option', { name: /anthropic · saved setup/i })
    ).toBeInTheDocument()

    fireEvent.change(providerSelect, { target: { value: 'provider-zhipu' } })

    await waitFor(() => {
      expect(screen.getByLabelText(/^saved ai service setup$/i)).toHaveValue('glm-4.7')
    })
  })

  test('submits cli kind without provider/model fields', async () => {
    selectProject()
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'CLI Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    const payload = createAgent.mock.calls[0][0]
    expect(payload).toMatchObject({
      kind: 'cli',
      name: 'CLI Worker',
      cliTool: 'claude',
      cwd: '/workspace',
      workspaceId: 'w1',
      projectId: 'p1',
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
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

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
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Name this agent before creating it.')
    )
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
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(
        'Open AI service settings, choose Check connection for this service, then come back when it shows Ready.'
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
    const submit = screen.getByRole('button', { name: /^add agent$/i })

    fireEvent.click(submit)
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Name this agent before creating it.')
    )
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(0))
    const callsAfterFirst = scrollSpy.mock.calls.length

    fireEvent.click(submit)
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(callsAfterFirst))
    expect(createAgent).not.toHaveBeenCalled()
    scrollSpy.mockRestore()
  })

  test('clipboard failure on the join screen shows a manual-copy message', async () => {
    selectProject()
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
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    const copyButton = await screen.findByRole('button', { name: /copy setup text/i })
    // jsdom has no navigator.clipboard, which is exactly the non-secure-context
    // (plain HTTP) deployment case the message exists for.
    fireEvent.click(copyButton)

    let alert: HTMLElement | null = null
    await waitFor(() =>
      expect((alert = screen.getByRole('alert'))).toHaveTextContent(
        'Copy did not work. Select the setup text in the box, then copy it yourself.'
      )
    )
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).not.toHaveTextContent(/clipboard access/i)
  })

  test('applies a starter template to simple chat agent instructions', async () => {
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
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    const templateGroup = screen.getByRole('group', { name: /agent starter templates/i })
    fireEvent.click(within(templateGroup).getByRole('button', { name: /check results/i }))

    expect(screen.getByLabelText(/^name$/i)).toHaveValue('Result Check Helper')
    expect((screen.getByLabelText(/agent instructions/i) as HTMLTextAreaElement).value).toContain(
      'confusing behavior'
    )
    expect((screen.getByLabelText(/agent instructions/i) as HTMLTextAreaElement).value).toContain(
      'clear use, fix, or wait recommendation'
    )
    expect((screen.getByLabelText(/agent instructions/i) as HTMLTextAreaElement).value).not.toMatch(
      /cite files|tradeoffs/i
    )
    expect(screen.queryByText(/^Reviewer$/)).toBeNull()
    expect(screen.queryByText(/prompt work/i)).toBeNull()
    expect(screen.queryByRole('group', { name: /agent role templates/i })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      kind: 'provider',
      name: 'Result Check Helper',
      provider: 'anthropic',
      model: 'claude-sonnet-4-6',
      systemPrompt: expect.stringContaining('clear use, fix, or wait recommendation'),
    })
  })

  test('uses plain wording in the investigation starter template', () => {
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
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    const templateGroup = screen.getByRole('group', { name: /agent starter templates/i })
    fireEvent.click(within(templateGroup).getByRole('button', { name: /find the cause/i }))

    const instructions = screen.getByLabelText(/agent instructions/i) as HTMLTextAreaElement
    expect(instructions.value).toContain('what the user already knows')
    expect(instructions.value).toContain('what is confirmed from what is only a guess')
    expect(instructions.value).toContain('next action that can confirm the answer')
    expect(instructions.value).not.toContain('gathering evidence first')
    expect(instructions.value).not.toContain('confirmed facts from guesses')
  })

  test('uses beginner-readable instructions in every starter template', () => {
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
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    const templateGroup = screen.getByRole('group', { name: /agent starter templates/i })
    const instructions = screen.getByLabelText(/agent instructions/i) as HTMLTextAreaElement

    fireEvent.click(within(templateGroup).getByRole('button', { name: /make a change/i }))
    expect(instructions.value).toContain('what changed and what to try next')
    expect(instructions.value).not.toMatch(/tradeoffs|edits narrow|handing work back/i)

    fireEvent.click(within(templateGroup).getByRole('button', { name: /check results/i }))
    expect(instructions.value).toContain('Explain each concern in plain language')
    expect(instructions.value).not.toMatch(/cite files|concrete risks/i)

    fireEvent.click(within(templateGroup).getByRole('button', { name: /find the cause/i }))
    expect(instructions.value).toContain('check the smallest useful clue next')
    expect(instructions.value).not.toContain('unclear failures')

    fireEvent.click(within(templateGroup).getByRole('button', { name: /fix a bug/i }))
    expect(instructions.value).toContain('what the user should try next')
    expect(instructions.value).not.toMatch(/fix the defect|failing case/i)
  })
})
