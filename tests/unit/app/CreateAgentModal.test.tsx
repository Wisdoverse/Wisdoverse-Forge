import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { CreateAgentModal } from '@app/features/agents/CreateAgentModal'
import { useAgentsStore } from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/entities/settings'
import { agentGroupApi } from '@app/entities/navigation/agent-group'

vi.mock('@app/entities/navigation/agent-group', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/entities/navigation/agent-group')>()

  return {
    ...actual,
    agentGroupApi: {
      getGroups: vi.fn().mockResolvedValue([]),
      createGroup: vi.fn().mockResolvedValue({
        id: 'group-new',
        name: 'Default Task Queue',
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

  function openRuntimeDetails() {
    fireEvent.click(screen.getByRole('button', { name: /why this option/i }))
    return screen.getByTestId('agent-runtime-fit')
  }

  function openStarterTemplates() {
    fireEvent.click(screen.getByRole('button', { name: /choose a starter template/i }))
    return screen.getByRole('group', { name: /agent starter templates/i })
  }

  test('links a Codex start failure to Work tool sign-ins', () => {
    render(<CreateAgentModal />)

    act(() => {
      useAgentsStore.setState({
        error:
          'Open Settings > Work tool sign-ins, sign in to Codex, then start this agent again. Agent was created and will stay offline in the list.',
      })
    })

    expect(screen.getByRole('alert')).toHaveTextContent('sign in to Codex')
    expect(screen.getByRole('link', { name: /open work tool sign-ins/i })).toHaveAttribute(
      'href',
      '/settings/work-tool-sign-ins'
    )
  })

  test('links a non-Codex start failure to AI service connections', () => {
    render(<CreateAgentModal />)

    act(() => {
      useAgentsStore.setState({
        error:
          'Open Settings > AI service connections, add or test the Anthropic connection, then start this agent again. Agent was created and will stay offline in the list.',
      })
    })

    expect(screen.getByRole('link', { name: /open ai service connections/i })).toHaveAttribute(
      'href',
      '/settings/providers'
    )
  })

  test('puts the agent type choice first and keeps detailed runtime help collapsed', () => {
    render(<CreateAgentModal />)

    const runtimeChoice = screen.getByText('Where should this agent work?')
    const starterTemplates = screen.getByRole('button', { name: /choose a starter template/i })

    expect(
      runtimeChoice.compareDocumentPosition(starterTemplates) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
    expect(screen.getByTestId('agent-kind-recommendation')).toHaveTextContent(
      /recommended: project files/i
    )
    expect(screen.queryByText(/not sure\?/i)).toBeNull()
    expect(screen.queryByTestId('agent-runtime-fit')).toBeNull()
    expect(screen.queryByRole('group', { name: /agent starter templates/i })).toBeNull()
  })

  test('shows each agent type capability before the user opens details', () => {
    render(<CreateAgentModal />)

    const choices = screen.getByRole('radiogroup', { name: /where should this agent work/i })

    expect(
      within(choices).getByText('Tasks and code changes in the selected project.')
    ).toBeInTheDocument()
    expect(
      within(choices).getByText('Tasks and code changes in a folder on this computer.')
    ).toBeInTheDocument()
    expect(
      within(choices).getByText('Questions only. It cannot take Tasks, change files, or use apps.')
    ).toBeInTheDocument()
    expect(screen.queryByTestId('agent-runtime-fit')).toBeNull()
  })

  test('renders project-file fields by default', () => {
    render(<CreateAgentModal />)

    expect(screen.getByRole('heading', { name: 'New agent' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: /project files/i })).toBeChecked()
    const detailsToggle = screen.getByRole('button', { name: /why this option/i })
    expect(detailsToggle).toHaveAttribute('aria-expanded', 'false')
    expect(screen.queryByTestId('agent-runtime-fit')).toBeNull()
    const starterToggle = screen.getByRole('button', { name: /choose a starter template/i })
    expect(starterToggle).toHaveAttribute('aria-expanded', 'false')
    expect(screen.queryByText('Pick a starter template')).toBeNull()
    expect(screen.queryByRole('group', { name: /agent starter templates/i })).toBeNull()
    expect(screen.queryByText('Start with a role')).toBeNull()
    expect(screen.getByText('Where should this agent work?')).toBeInTheDocument()
    expect(screen.getByRole('radiogroup', { name: /where should this agent work/i })).toBeDefined()
    expect(
      screen.getByText(/start with what this agent should be allowed to touch/i)
    ).toBeInTheDocument()
    expect(
      screen.queryByText(
        /Use this for the usual setup when the agent should edit shared project files/i
      )
    ).toBeNull()
    expect(screen.getByText('Best for project changes')).toBeInTheDocument()
    expect(screen.queryByText('Most file work')).toBeNull()
    expect(
      screen.queryByText(/Use this when files or tools must stay on this computer/i)
    ).toBeNull()
    expect(screen.getByText('Local files')).toBeInTheDocument()
    expect(
      screen.queryByText(
        /Use this for questions and result checks in chat. It cannot take Tasks, change files, or use apps/i
      )
    ).toBeNull()
    expect(screen.getByText('Questions only')).toBeInTheDocument()
    expect(screen.queryByText('Chat only')).toBeNull()
    expect(screen.queryByText('Choose work style')).toBeNull()
    expect(screen.queryByText('Fills in the name and first task')).toBeNull()
    expect(screen.queryByText('Updates the work and checks it')).toBeNull()
    expect(screen.queryByText('Builds changes and checks them')).toBeNull()
    expect(screen.getAllByText(/claude with project files/i).length).toBeGreaterThan(0)
    expect(screen.queryByText('Project files are included')).toBeNull()
    expect(screen.queryByText('Shared project folder')).toBeNull()
    expect(screen.queryByText('Check Where agents work in Settings')).toBeNull()
    expect(screen.queryByText('Can edit files')).toBeNull()
    fireEvent.click(detailsToggle)
    expect(detailsToggle).toHaveAttribute('aria-expanded', 'true')
    const runtimeDetails = screen.getByTestId('agent-runtime-fit')
    expect(within(runtimeDetails).getByText('Project files are included')).toBeInTheDocument()
    expect(within(runtimeDetails).getByText('Where it works')).toBeInTheDocument()
    expect(within(runtimeDetails).getByText('Shared project folder')).toBeInTheDocument()
    expect(
      within(runtimeDetails).getByText('Check Where agents work in Settings')
    ).toBeInTheDocument()
    expect(within(runtimeDetails).getByText('Can edit files')).toBeInTheDocument()
    expect(screen.getByTestId('agent-kind-recommendation')).toHaveTextContent(
      /recommended: project files/i
    )
    expect(screen.queryByText(/not sure\?/i)).toBeNull()
    expect(screen.queryByText(/ready workspace managed by forge/i)).toBeNull()
    expect(screen.queryByText(/Forge project area/i)).toBeNull()
    expect(screen.queryByText(/agent location/i)).toBeNull()
    expect(screen.queryByText(/workspace must be ready/i)).toBeNull()
    expect(screen.queryByText(/choose a runtime/i)).toBeNull()
    expect(screen.queryByText('File work')).toBeNull()
    expect(screen.getByRole('combobox', { name: /^tool for file changes$/i })).toBeInTheDocument()
    expect(screen.queryByRole('combobox', { name: /^work tool$/i })).toBeNull()
    expect(
      screen.getByText(/which tool this team uses to change project files/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/which work tool this team uses/i)).toBeNull()
    expect(screen.getByLabelText(/work folder/i)).toBeInTheDocument()
    expect(screen.getByText(/keep the suggested folder/i)).toBeInTheDocument()
    expect(screen.getByText(/new tasks start from the project shown above/i)).toBeInTheDocument()
    expect(screen.queryByText(/use \/workspace unless/i)).toBeNull()
    expect(screen.queryByText(/default task context/i)).toBeNull()
    expect(screen.getAllByText(/project for new tasks/i).length).toBeGreaterThan(0)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/open project settings/i)
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(
      /open project settings to create or choose a project before creating this project files agent/i
    )
    expect(screen.getByTestId('agent-work-readiness')).not.toHaveTextContent(/file-working/i)
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

    fireEvent.click(starterToggle)
    expect(starterToggle).toHaveAttribute('aria-expanded', 'true')
    const templateGroup = screen.getByRole('group', { name: /agent starter templates/i })
    expect(screen.getByText('Fills in the name and how this agent should work')).toBeInTheDocument()
    expect(screen.queryByText('Fills in the name and first task')).toBeNull()
    expect(within(templateGroup).getByText('Updates the work and checks it')).toBeInTheDocument()
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

  test('uses the primary button to open project settings before project files agent creation', () => {
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
      within(review).getByText('Set up a place here when you want new tasks to wait together.')
    ).toBeInTheDocument()
  })

  test('guides users when places for new tasks exist but none is selected yet', () => {
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
      within(review).getByText('Choose a place for new tasks now, or set it later from Tasks.')
    ).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Review place' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: /set this later/i })).toBeInTheDocument()
    expect(within(review).getByText('Place for new tasks')).toBeInTheDocument()
    expect(within(review).queryByText('No task queue selected yet')).toBeNull()
    expect(screen.queryByRole('option', { name: /^no task queue$/i })).toBeNull()
    expect(screen.queryByRole('option', { name: 'Review Queue' })).toBeNull()
    expect(screen.queryByText('Where tasks wait')).toBeNull()

    fireEvent.change(screen.getByLabelText(/place for new tasks/i), {
      target: { value: 'group-1' },
    })

    expect(within(review).getByText('Review place')).toBeInTheDocument()
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
      screen.getByText(/starter place for this project so new tasks have somewhere to wait/i)
    ).toBeInTheDocument()
    fireEvent.click(await screen.findByRole('button', { name: /set up place/i }))

    await waitFor(() =>
      expect(agentGroupApi.createGroup).toHaveBeenCalledWith({
        projectId: 'p1',
        name: 'Default Task Queue',
        description:
          'Starter place for this project. New tasks wait here until an agent can take them.',
      })
    )
    expect(screen.getByRole('combobox', { name: /place for new tasks/i })).toHaveValue('group-new')
    expect(screen.getByTestId('agent-work-readiness')).toHaveTextContent(/project ready/i)
    expect(
      screen.getByText(/new tasks wait here until an available agent can take them/i)
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('agent-create-review')).getByText('Default place')
    ).toBeInTheDocument()
    expect(screen.queryByText('Where tasks wait')).toBeNull()
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

  test('hides raw task-queue creation errors while setting up the default queue', async () => {
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
    fireEvent.click(await screen.findByRole('button', { name: /set up place/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'Wait a few minutes, then set up the place for new tasks again.'
    )
    expect(alert).toHaveTextContent('ask an owner or admin to check places in this project')
    expect(alert).not.toHaveTextContent('task queue')
    expect(alert).not.toHaveTextContent('task routing setup')
    expect(alert).not.toHaveTextContent('waiting place')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('database unavailable')
  })

  test('switching to simple chat agent hides file-change tool fields and shows AI service details', () => {
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

    expect(screen.queryByText('Fills in the name and how this agent should work')).toBeNull()
    expect(
      screen.getAllByText(/anthropic for questions and result checks/i).length
    ).toBeGreaterThan(0)
    expect(
      screen.getAllByText(/cannot take Tasks, change files, or use apps/i).length
    ).toBeGreaterThan(0)
    expect(screen.getByTestId('simple-chat-limits')).toHaveTextContent(
      /simple chat answers questions only/i
    )
    expect(screen.getByTestId('simple-chat-limits')).not.toHaveTextContent(/task runner/i)
    expect(screen.getByTestId('simple-chat-limits')).not.toHaveTextContent(/run commands/i)
    expect(screen.getByText(/use it for questions and result checks/i)).toBeInTheDocument()
    expect(screen.queryByText(/file work/i)).toBeNull()
    expect(screen.queryByText(/does not open project files/i)).toBeNull()
    expect(screen.queryByText('Check AI services in Settings')).toBeNull()
    const runtimeDetails = openRuntimeDetails()
    expect(runtimeDetails).toHaveTextContent(/does not open project files/i)
    expect(runtimeDetails).toHaveTextContent(/questions, writing, and checking results/i)
    expect(within(runtimeDetails).getByText('Check AI services in Settings')).toBeInTheDocument()
    expect(within(runtimeDetails).queryByText('Check AI service in Settings')).toBeNull()
    expect(screen.queryByText(/ai service must be checked/i)).toBeNull()
    expect(screen.queryByRole('combobox', { name: /^tool for file changes$/i })).toBeNull()
    expect(screen.queryByRole('combobox', { name: /^work tool$/i })).toBeNull()
    expect(screen.queryByLabelText(/work folder/i)).toBeNull()
    expect(screen.getByLabelText(/^ai service$/i)).toHaveValue('provider-anthropic')
    expect(screen.getByText('Answer setting from Settings')).toBeInTheDocument()
    expect(screen.queryByDisplayValue('claude-sonnet-4-6')).toBeNull()
    expect(screen.queryByLabelText(/^saved ai service choice$/i)).toBeNull()
    expect(screen.queryByLabelText(/^ai model$/i)).toBeNull()
    expect(screen.getByText(/choose the AI service you set up/i)).toBeInTheDocument()
    expect(
      screen.getByText(/forge uses the answer setting that is already checked in Settings/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/you do not need to choose anything else here/i)).toBeInTheDocument()
    expect(screen.queryByText(/saved service choice/i)).toBeNull()
    expect(screen.queryByText(/filled from AI service settings/i)).toBeNull()
    expect(screen.queryByText(/AI services settings/i)).toBeNull()
    const readiness = screen.getByTestId('agent-work-readiness')
    expect(within(readiness).getByText('Where to use it')).toBeInTheDocument()
    expect(
      within(readiness).getByText(
        'Open this agent from Chat. It does not need a project or a place for new tasks.'
      )
    ).toBeInTheDocument()
    expect(within(readiness).queryByText('Shown under project')).toBeNull()
    expect(within(readiness).queryByText('Choose a project later')).toBeNull()
    expect(within(readiness).queryByText('Project for new tasks')).toBeNull()
    const review = screen.getByTestId('agent-create-review')
    expect(within(review).getByText('How to use it')).toBeInTheDocument()
    expect(
      within(review).getByText('Open this agent from Chat. It does not take Tasks.')
    ).toBeInTheDocument()
    expect(within(review).getByText('Tasks and files')).toBeInTheDocument()
    expect(
      within(review).getByText(
        'Need Tasks or code changes? Create an agent with Project files or This computer.'
      )
    ).toBeInTheDocument()
    expect(
      within(review).queryByText(
        'Need Tasks or code changes? Create a Project files or This computer agent.'
      )
    ).toBeNull()
    expect(
      within(review).getByText(
        'Ready for questions and result checks after the AI service is connected.'
      )
    ).toBeInTheDocument()
    expect(within(review).queryByText('Project for new tasks')).toBeNull()
    expect(within(review).queryByText('Task queue')).toBeNull()
    expect(within(review).queryByText('Not used by Simple chat agents.')).toBeNull()
    expect(screen.queryByLabelText(/^model name$/i)).toBeNull()
    expect(screen.queryByLabelText(/^provider$/i)).toBeNull()
    expect(screen.queryByLabelText(/^model$/i)).toBeNull()

    const templateGroup = openStarterTemplates()
    expect(screen.getByText('Fills in the name and how this agent should work')).toBeInTheDocument()
    expect(within(templateGroup).getByText('Check results')).toBeInTheDocument()
  })

  test('simple chat agent hides task queues and does not submit a task queue', async () => {
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

    fireEvent.click(screen.getByRole('radio', { name: /project files/i }))
    fireEvent.change(screen.getByLabelText(/place for new tasks/i), {
      target: { value: 'group-1' },
    })

    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))

    expect(screen.queryByLabelText(/task queue/i)).toBeNull()
    expect(screen.queryByRole('button', { name: /set up task queue/i })).toBeNull()
    expect(screen.queryByLabelText(/place for new tasks/i)).toBeNull()
    expect(screen.queryByRole('button', { name: /set up place/i })).toBeNull()
    expect(
      within(screen.getByTestId('agent-work-readiness')).getByText('Where to use it')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('agent-work-readiness')).getByText(
        'Open this agent from Chat. It does not need a project or a place for new tasks.'
      )
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('agent-create-review')).getByText(
        'Need Tasks or code changes? Create an agent with Project files or This computer.'
      )
    ).toBeInTheDocument()
    expect(within(screen.getByTestId('agent-create-review')).queryByText('Task queue')).toBeNull()

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Question Helper' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      kind: 'provider',
      provider: 'anthropic',
      model: 'claude-sonnet-4-6',
      projectId: 'p1',
    })
    expect(createAgent.mock.calls[0][0]).not.toHaveProperty('groupId')
  })

  test('simple chat agent with no checked AI service shows a Settings hint and blocks submit', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))

    expect(screen.queryByTestId('agent-runtime-fit')).toBeNull()
    const runtimeDetails = openRuntimeDetails()
    expect(runtimeDetails).toHaveTextContent(/AI service for questions and result checks/i)
    expect(runtimeDetails).not.toHaveTextContent(/Provider for questions and result checks/i)
    const providerHint = screen.getByTestId('provider-empty-hint')
    expect(providerHint).toHaveTextContent(/add and check an ai service first/i)
    expect(providerHint).toHaveTextContent(/open ai services in settings/i)
    expect(providerHint).toHaveTextContent(/paste the key from that service/i)
    expect(providerHint).toHaveTextContent(/choose check connection/i)
    expect(providerHint).toHaveTextContent(/service shows ready/i)
    expect(providerHint).not.toHaveTextContent(/open ai service settings/i)
    expect(providerHint).not.toHaveTextContent(/no ai service ready yet/i)
    expect(providerHint).not.toHaveTextContent(/service access key/i)
    expect(providerHint).not.toHaveTextContent(/settings > ai services/i)
    expect(providerHint).not.toHaveTextContent(/ai services settings/i)
    expect(providerHint).not.toHaveTextContent(/click check/i)
    expect(screen.getByRole('link', { name: /open ai services/i })).toHaveAttribute(
      'href',
      '/settings/providers'
    )
    expect(screen.queryByLabelText(/^ai service$/i)).toBeNull()

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    let alert: HTMLElement | null = null
    await waitFor(() =>
      expect((alert = screen.getByRole('alert'))).toHaveTextContent(/open ai services in settings/i)
    )
    expect(alert).toHaveTextContent(/paste the key from that service/i)
    expect(alert).toHaveTextContent(/come back when it shows ready/i)
    expect(alert).not.toHaveTextContent(/open ai service settings/i)
    expect(alert).not.toHaveTextContent(/service access key/i)
    expect(alert).not.toHaveTextContent(/until it says ready/i)
    expect(alert).not.toHaveTextContent(/settings > ai services/i)
    expect(alert).not.toHaveTextContent(/ai services settings/i)
    expect(alert).not.toHaveTextContent(/click check/i)
    expect(
      within(alert as HTMLElement).getByRole('link', { name: /open ai services/i })
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
    fireEvent.change(screen.getByRole('combobox', { name: /^tool for file changes$/i }), {
      target: { value: 'codex' },
    })
    expect(screen.getAllByText(/codex with project files/i).length).toBeGreaterThan(0)

    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    expect(screen.getAllByText(/codex on this computer/i).length).toBeGreaterThan(0)
    expect(screen.getByTestId('agent-kind-recommendation')).toHaveTextContent(
      /files must stay on this computer/i
    )
    expect(screen.queryByText(/After setup, Forge still manages this agent here/i)).toBeNull()
    expect(screen.queryByText(/Forge gives it tasks/i)).toBeNull()
    expect(screen.queryByText('Paste setup text in Terminal or PowerShell')).toBeNull()
    const localRuntimeDetails = openRuntimeDetails()
    expect(localRuntimeDetails).toHaveTextContent(
      /After setup, Forge still manages this agent here/i
    )
    expect(localRuntimeDetails).toHaveTextContent(/tasks, status, and task history/i)
    expect(
      within(localRuntimeDetails).getByText('Follow the setup steps shown after creation')
    ).toBeInTheDocument()
    expect(localRuntimeDetails).not.toHaveTextContent(/Terminal or PowerShell/)
    expect(within(localRuntimeDetails).getByText('Uses this computer')).toBeInTheDocument()
    const localNextStep = screen.getByTestId('local-agent-before-create')
    expect(localNextStep).toHaveTextContent('Before you create this computer agent')
    expect(localNextStep).toHaveTextContent(
      'Choose the folder this computer should work in. If you are not sure, leave it blank.'
    )
    expect(localNextStep).toHaveTextContent(
      'After you choose Add agent, Forge shows the setup text and the app to paste it into.'
    )
    expect(localNextStep).not.toHaveTextContent(/Terminal or PowerShell/)
    expect(localNextStep).toHaveTextContent(
      'Success looks like this agent changing to Ready on the Agents page.'
    )
    expect(localNextStep).not.toHaveTextContent(/manual connection/i)
    expect(localNextStep).not.toHaveTextContent(/runtime/i)
    const localReview = screen.getByTestId('agent-create-review')
    expect(
      within(localReview).getByText(
        'Follow the setup steps shown after creation and keep that window open.'
      )
    ).toBeInTheDocument()
    expect(localReview).not.toHaveTextContent(/Terminal or PowerShell/)
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
    expect(screen.queryByTestId('agent-runtime-fit')).toBeNull()
    const providerRuntimeDetails = openRuntimeDetails()
    expect(within(providerRuntimeDetails).getByText('AI service only')).toBeInTheDocument()
    expect(
      within(providerRuntimeDetails).getByText('Check AI services in Settings')
    ).toBeInTheDocument()
    expect(within(providerRuntimeDetails).queryByText('Check AI service in Settings')).toBeNull()
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
    fireEvent.change(screen.getByRole('combobox', { name: /tool for files on this computer/i }), {
      target: { value: 'codex' },
    })
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Laptop Worker' } })
    fireEvent.change(screen.getByRole('textbox', { name: /folder on this computer/i }), {
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
    expect(screen.getByText(/Open the setup app shown above on that computer/i)).toBeInTheDocument()
    expect(
      screen.getByText(/Forge will manage its tasks, status, and history/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/files stay on that computer/i)).toBeInTheDocument()
    expect(screen.getByText('1. Copy the setup text.')).toBeInTheDocument()
    expect(screen.getByText(/paste it into the setup app shown above/i)).toBeInTheDocument()
    expect(
      screen.getByText(/changes from Not connected to Ready on the Agents page/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/Closing that window disconnects this agent/i)).toBeInTheDocument()
    expect(screen.getByText(/come back to Forge, open Agents/i)).toBeInTheDocument()
    expect(screen.queryByText(/command app/i)).toBeNull()
    expect(screen.queryByText(/Terminal or PowerShell/i)).toBeNull()
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
      'Open the setup app for macOS or Linux, then paste this setup text.'
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
      'Open the setup app for Windows, then paste this setup text.'
    )
    expect(screen.queryByText(/Terminal|PowerShell/)).toBeNull()

    // Backup setup text stays out of the default path until the user needs it.
    expect(screen.getByText(/if the setup text does not work/i)).toBeInTheDocument()
    expect(screen.queryByText(/Use this backup only/i)).toBeNull()
    expect(screen.queryByRole('button', { name: /copy backup setup/i })).toBeNull()
    expect(screen.queryByLabelText(/backup setup text/i)).toBeNull()

    fireEvent.click(screen.getByText(/if the setup text does not work/i))

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
    expect(screen.queryByText(/Open PowerShell/i)).toBeNull()
    expect(screen.queryByText(/keep PowerShell open/i)).toBeNull()
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
    expect(screen.queryByRole('combobox', { name: /^tool for file changes$/i })).toBeNull()
    expect(screen.queryByRole('combobox', { name: /^work tool$/i })).toBeNull()
    expect(screen.getByLabelText(/^ai service$/i)).toHaveValue('provider-1')
    expect(screen.getByText('Answer setting from Settings')).toBeInTheDocument()
    expect(screen.queryByDisplayValue('gpt-5.5')).toBeNull()
    expect(
      within(screen.getByTestId('agent-work-readiness')).getByText('Where to use it')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('agent-work-readiness')).queryByText('Choose a project later')
    ).toBeNull()
    const chatLimits = screen.getByTestId('simple-chat-limits')
    expect(chatLimits).toHaveTextContent('Simple chat answers questions only')
    expect(chatLimits).toHaveTextContent(
      'It can answer questions and check text in chat. It cannot take Tasks, edit files, or use apps.'
    )
    expect(chatLimits).toHaveTextContent(
      'Need Tasks or code changes? Choose Project files for shared project files, or This computer for local files and apps.'
    )
    expect(chatLimits).not.toHaveTextContent('provider runtime')

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
    expect(screen.queryByDisplayValue('gpt-5.4')).toBeNull()

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Provider Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    await waitFor(() => expect(createAgent).toHaveBeenCalledTimes(1))
    expect(createAgent.mock.calls[0][0]).toMatchObject({
      provider: 'openai',
      model: 'gpt-5.4',
    })
  })

  test('lists configured providers (including China-region) by display name and ready Settings status', async () => {
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
    // Each option shows the display name and keeps the technical model name out of the flow.
    expect(
      within(providerSelect).getByRole('option', { name: /zhipu glm · ready in settings/i })
    ).toBeInTheDocument()
    expect(
      within(providerSelect).getByRole('option', { name: /anthropic · ready in settings/i })
    ).toBeInTheDocument()
    expect(
      within(providerSelect).queryByRole('option', { name: /saved service choice/i })
    ).not.toBeInTheDocument()
    expect(
      within(providerSelect).queryByRole('option', { name: /glm-4.7/i })
    ).not.toBeInTheDocument()

    fireEvent.change(providerSelect, { target: { value: 'provider-zhipu' } })

    expect(screen.getByText('Answer setting from Settings')).toBeInTheDocument()
    expect(screen.queryByDisplayValue('glm-4.7')).toBeNull()
  })

  test('does not mark unchecked AI services as ready when creating a simple chat agent', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)
    useSettingsStore.setState({
      providers: [
        {
          id: 'provider-unchecked',
          provider: 'openai',
          displayName: 'OpenAI Draft',
          model: 'gpt-5.5',
          priority: 1,
          isEnabled: true,
          isDefault: true,
          lastTestStatus: 'untested',
        },
      ],
    })

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))

    const providerSelect = screen.getByLabelText(/^ai service$/i)
    expect(
      within(providerSelect).getByRole('option', { name: /openai draft · check connection first/i })
    ).toBeInTheDocument()
    expect(within(providerSelect).queryByRole('option', { name: /ready in settings/i })).toBeNull()
    expect(screen.getByText(/choose check connection in settings before creating/i)).toBeDefined()
    expect(screen.getByText('Answer setting from Settings')).toBeInTheDocument()
    expect(screen.getByText('Check connection first')).toBeInTheDocument()
    expect(screen.queryByText(/^Ready$/)).toBeNull()

    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Chat Helper' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'Open AI services in Settings, choose Check connection for this service, then come back when it shows Ready.'
    )
    expect(alert).not.toHaveTextContent('Open AI service settings')
    expect(within(alert).getByRole('link', { name: /open ai services/i })).toHaveAttribute(
      'href',
      '/settings/providers'
    )
    expect(createAgent).not.toHaveBeenCalled()
  })

  test('links this-computer setup failures to Where agents work settings', async () => {
    selectProject()
    const enrollLocalAgent = vi.fn(async () => {
      useAgentsStore.setState({
        loading: false,
        error:
          'Wait a moment, then open Agents and choose New agent again. Forge could not prepare the setup text for this computer right now. If it still fails, ask an owner or admin to check Where agents work in Settings.',
      })
      return null
    })
    useAgentsStore.setState({ enrollLocalAgent } as never)

    render(<CreateAgentModal />)

    fireEvent.click(screen.getByRole('radio', { name: /this computer/i }))
    fireEvent.change(screen.getByLabelText(/^name$/i), { target: { value: 'Laptop Worker' } })
    fireEvent.click(screen.getByRole('button', { name: /^add agent$/i }))

    await waitFor(() => expect(enrollLocalAgent).toHaveBeenCalledTimes(1))
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(/could not prepare the setup text for this computer/i)
    expect(within(alert).getByRole('link', { name: /open where agents work/i })).toHaveAttribute(
      'href',
      '/settings/runtime'
    )
    expect(within(alert).queryByRole('link', { name: /open ai service settings/i })).toBeNull()
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
        'Open AI services in Settings, choose Check connection for this service, then come back when it shows Ready.'
      )
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent('Open AI service settings')
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
    const templateGroup = openStarterTemplates()
    fireEvent.click(within(templateGroup).getByRole('button', { name: /check results/i }))

    expect(screen.getByLabelText(/^name$/i)).toHaveValue('Result Check Helper')
    expect(
      (screen.getByLabelText(/tell this agent how to answer/i) as HTMLTextAreaElement).value
    ).toContain('confusing behavior')
    expect(
      (screen.getByLabelText(/tell this agent how to answer/i) as HTMLTextAreaElement).value
    ).toContain('clear use, fix, or wait recommendation')
    expect(
      (screen.getByLabelText(/tell this agent how to answer/i) as HTMLTextAreaElement).value
    ).not.toMatch(/cite files|tradeoffs/i)
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
    const templateGroup = openStarterTemplates()
    fireEvent.click(within(templateGroup).getByRole('button', { name: /find the cause/i }))

    const instructions = screen.getByLabelText(
      /tell this agent how to answer/i
    ) as HTMLTextAreaElement
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
    const templateGroup = openStarterTemplates()
    const instructions = screen.getByLabelText(
      /tell this agent how to answer/i
    ) as HTMLTextAreaElement
    expect(within(templateGroup).getByText('Checks the next useful clue')).toBeInTheDocument()
    expect(within(templateGroup).queryByText('Tracks down unclear failures')).toBeNull()
    expect(within(templateGroup).getByText('Checks the problem and fixes it')).toBeInTheDocument()
    expect(within(templateGroup).queryByText('Reproduces and fixes bugs')).toBeNull()

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
