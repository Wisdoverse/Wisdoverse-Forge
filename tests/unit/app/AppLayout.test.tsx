import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react'
import { i18n } from '@app/i18n'
import { useBoardStore } from '@app/shared/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/shared/model/settings.store'

const mockGetParticipants = vi.fn()
const mockCreateTask = vi.fn()
const mockGetGroups = vi.fn()
const mockCreateGroup = vi.fn()
const routerState = vi.hoisted(() => ({ path: '/tasks' }))
const originalSetGettingStartedDismissed = useSettingsStore.getState().setGettingStartedDismissed

vi.mock('@app/shared/model/auth.context', () => ({
  useAuth: () => ({
    authManager: { logout: vi.fn() },
    user: { role: 'user' },
    isAuthenticated: true,
    isLoading: false,
  }),
}))

vi.mock('@tanstack/react-router', () => ({
  useRouterState: ({ select }: { select: (s: any) => string }) =>
    select({ location: { pathname: routerState.path } }),
  useNavigate: () => vi.fn(),
}))

vi.mock('@app/shared/api/orchestration', () => ({
  taskResultArtifacts: (result: unknown) => (Array.isArray(result) ? result : []),
  orchestrationApi: {
    getParticipants: (...args: unknown[]) => mockGetParticipants(...args),
    createTask: (...args: unknown[]) => mockCreateTask(...args),
  },
}))

vi.mock('@app/entities/agent-group', () => ({
  agentGroupApi: {
    getGroups: (...args: unknown[]) => mockGetGroups(...args),
    createGroup: (...args: unknown[]) => mockCreateGroup(...args),
  },
  waitingPlaceDisplayName: (name: string | null | undefined) => name || 'this place',
}))

import { MemoryRouter } from './layout-test-wrapper'

beforeEach(() => {
  routerState.path = '/tasks'
  useNavigationStore.getState().reset()
  mockGetParticipants.mockResolvedValue([
    { id: 'participant-1', agentId: 'agent-1', name: 'Agent One', status: 'available' },
  ])
  mockGetGroups.mockResolvedValue([{ id: 'group-1', name: 'Default', projectId: 'p1' }])
  mockCreateGroup.mockResolvedValue({ id: 'group-new', name: 'Frontend', projectId: 'p1' })
  mockCreateTask.mockResolvedValue({
    ok: true,
    task: {
      id: 'modal-task-1',
      groupId: 'group-1',
      state: 'backlog',
      method: 'tasks/send',
      params: { task: 'Modal task', message: 'Details' },
      assignedTo: 'agent-1',
      assignedAgentName: 'Agent One',
      priority: 'high',
      progress: 0,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
  })
})

afterEach(async () => {
  cleanup()
  useBoardStore.getState().reset()
  useNavigationStore.getState().reset()
  useSettingsStore.setState({
    preferences: null,
    preferencesLoaded: false,
    setGettingStartedDismissed: originalSetGettingStartedDismissed,
  })
  vi.clearAllMocks()
  Object.defineProperty(window, 'innerWidth', { value: 1024, configurable: true })
  await i18n.changeLanguage('en')
})

function seedProjectNavigation(selectedProjectId: string | null = 'p1') {
  useNavigationStore.setState({
    orgs: [{ id: 'org1', name: 'Org', slug: 'org', plan: 'pro', role: 'owner' }],
    selectedOrgId: 'org1',
    teams: [
      {
        id: 't1',
        orgId: 'org1',
        name: 'Team Alpha',
        slug: 'team-alpha',
        visibility: 'open',
        description: '',
      },
    ],
    projects: {
      t1: [
        {
          id: 'p1',
          teamId: 't1',
          name: 'Project X',
          slug: 'project-x',
          color: '#007AFF',
          description: '',
        },
      ],
    },
    expandedTeams: ['t1'],
    selectedProjectId,
  })
}

describe('AppLayout', () => {
  test('renders sidebar, top bar, main content area, and right panel toggle', () => {
    render(<MemoryRouter />)
    expect(screen.getByTestId('sidebar')).toBeDefined()
    expect(screen.getByTestId('top-bar')).toBeDefined()
    expect(screen.getByTestId('main-content')).toBeDefined()
    // The updates area defaults to collapsed; assert the reveal affordance instead.
    const revealButton = screen.getByRole('button', { name: /show live task updates/i })
    expect(revealButton).toBeDefined()
    expect(within(revealButton).getByText('Activity')).toBeDefined()
  })

  test('empty live updates can open the task board', () => {
    routerState.path = '/settings'
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.click(screen.getByRole('button', { name: /show live task updates/i }))
    fireEvent.click(screen.getByRole('button', { name: /open task board/i }))

    expect(onNavigate).toHaveBeenCalledWith('/tasks')
  })

  test('sidebar has navigation items', () => {
    render(<MemoryRouter />)
    const navItems = screen.getAllByTestId(/^sidebar-nav-/)
    expect(navItems.length).toBeGreaterThanOrEqual(1)
    expect(screen.queryByTestId('sidebar-nav-start')).toBeNull()
    expect(screen.getByTestId('sidebar-nav-tasks')).toBeDefined()
  })

  test('top bar shows view toggles', () => {
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')

    render(<MemoryRouter />)
    expect(screen.getByText('Board')).toBeDefined()
    expect(screen.getByText('List')).toBeDefined()
    expect(screen.getByText('Timeline')).toBeDefined()
    expect(screen.getByText('Map')).toBeDefined()
    expect(screen.getByRole('button', { name: /new task/i })).toBeDefined()
    expect(screen.queryByRole('button', { name: '3D' })).toBeNull()
    expect(screen.queryByRole('button', { name: /\+ task/i })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Status' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Agent' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Priority' })).toBeNull()
  })

  test('localizes the top bar and page title for Chinese beginners', async () => {
    await i18n.changeLanguage('zh')
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: '任务' })).toBeDefined()
    expect(screen.getByText('创建任务，并跟进智能体进度')).toBeDefined()
    expect(screen.getByRole('button', { name: '搜索页面和可做的事' })).toBeDefined()
    expect(screen.getByText('搜索')).toBeDefined()
    expect(screen.getByRole('button', { name: '切换到深色模式' })).toBeDefined()
    expect(screen.getByRole('button', { name: '看板' })).toBeDefined()
    expect(screen.getByRole('button', { name: '列表' })).toBeDefined()
    expect(screen.getByRole('button', { name: '时间线' })).toBeDefined()
    expect(screen.getByRole('button', { name: '地图' })).toBeDefined()
    const taskButton = screen.getByRole('button', { name: '新任务' })
    expect(taskButton).toHaveAttribute('title', '让智能体完成一项任务。')
    expect(screen.queryByRole('heading', { name: 'Tasks' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Search pages and things to do' })).toBeNull()
  })

  test('top bar shows the setup step instead of New task when no project exists', () => {
    render(<MemoryRouter />)

    const setupButton = screen.getByRole('button', { name: 'Set up project' })
    expect(setupButton).toHaveAttribute(
      'title',
      'Open project settings so tasks have a place to belong.'
    )
    expect(screen.queryByRole('button', { name: /^new task$/i })).toBeNull()
  })

  test('top bar shows waiting-place setup before task creation when project setup is incomplete', () => {
    seedProjectNavigation('p1')

    render(<MemoryRouter />)

    const setupButton = screen.getByRole('button', { name: 'Set up waiting place' })
    expect(setupButton).toHaveAttribute(
      'title',
      'Open Agents to add a waiting place before creating a task.'
    )
    expect(screen.queryByRole('button', { name: /^new task$/i })).toBeNull()
  })

  test('top bar labels the command search entry for beginners', () => {
    render(<MemoryRouter />)

    const searchButton = screen.getByTestId('top-bar-command-search')
    expect(searchButton).toHaveAccessibleName('Search pages and things to do')
    expect(screen.getByText('Search')).toBeDefined()

    fireEvent.click(searchButton)

    expect(screen.getByPlaceholderText(/search pages or things to do/i)).toBeDefined()
    expect(
      screen.queryByPlaceholderText(new RegExp(['search', 'commands'].join('\\s+'), 'i'))
    ).toBeNull()
  })

  test('command palette New task action opens the task form', async () => {
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')

    render(<MemoryRouter />)

    fireEvent.click(screen.getByTestId('top-bar-command-search'))
    fireEvent.click(screen.getByText('Create a task for an agent to finish.'))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    expect(screen.getByRole('dialog')).toBeDefined()
    expect(screen.getByLabelText(/what should the agent finish/i)).toBeDefined()
    expect(screen.queryByPlaceholderText(/search pages or things to do/i)).toBeNull()
  })

  test('command palette routes first task setup to project settings when no project exists', async () => {
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.click(screen.getByTestId('top-bar-command-search'))
    await waitFor(() => {
      expect(screen.getByText('Set up project before task')).toBeDefined()
    })
    fireEvent.click(screen.getByText('Open project settings so tasks have a place to belong.'))

    expect(onNavigate).toHaveBeenCalledWith('/settings/projects')
    expect(screen.queryByPlaceholderText(/search pages or things to do/i)).toBeNull()
    expect(screen.queryByRole('dialog', { name: /tell an agent what to do/i })).toBeNull()
  })

  test('command palette keeps first task setup copy readable in Chinese', async () => {
    await i18n.changeLanguage('zh')
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.click(screen.getByTestId('top-bar-command-search'))
    await waitFor(() => {
      expect(screen.getByText('创建任务前先设置项目')).toBeDefined()
    })
    fireEvent.click(screen.getByText('打开项目设置，让任务有归属位置。'))

    expect(onNavigate).toHaveBeenCalledWith('/settings/projects')
    expect(screen.queryByPlaceholderText('搜索页面或要做的事，例如：任务、收件箱、设置')).toBeNull()
  })

  test('command palette routes task setup to Agents when tasks have nowhere to wait', async () => {
    seedProjectNavigation('p1')
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.click(screen.getByTestId('top-bar-command-search'))
    await waitFor(() => {
      expect(screen.getByText('Set up where tasks wait')).toBeDefined()
    })
    fireEvent.click(screen.getByText('Open Agents to add a waiting place before creating a task.'))

    expect(onNavigate).toHaveBeenCalledWith('/agents')
    expect(screen.queryByPlaceholderText(/search pages or things to do/i)).toBeNull()
    expect(screen.queryByRole('dialog', { name: /tell an agent what to do/i })).toBeNull()
  })

  test('command palette task view actions open Tasks before switching view', () => {
    routerState.path = '/settings'
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.click(screen.getByTestId('top-bar-command-search'))
    fireEvent.click(screen.getByText('Scan tasks in one sortable table.'))

    expect(onNavigate).toHaveBeenCalledWith('/tasks')
    expect(useBoardStore.getState().viewMode).toBe('list')
    expect(screen.queryByPlaceholderText(/search pages or things to do/i)).toBeNull()
  })

  test('project menu New task opens the task form when the project has a waiting place', async () => {
    seedProjectNavigation(null)
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.contextMenu(screen.getByTestId('project-p1'))
    fireEvent.click(screen.getByRole('menuitem', { name: /new task for this project/i }))

    await waitFor(() => expect(mockGetGroups).toHaveBeenCalledWith('p1'))
    expect(onNavigate).toHaveBeenCalledWith('/tasks')
    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    expect(screen.getByRole('dialog', { name: /tell an agent what to do/i })).toBeDefined()
  })

  test('project menu New task routes to Agents when the project has no waiting place', async () => {
    seedProjectNavigation(null)
    mockGetGroups.mockResolvedValueOnce([])
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.contextMenu(screen.getByTestId('project-p1'))
    fireEvent.click(screen.getByRole('menuitem', { name: /new task for this project/i }))

    await waitFor(() => expect(mockGetGroups).toHaveBeenCalledWith('p1'))
    expect(onNavigate).toHaveBeenCalledWith('/agents')
    expect(screen.queryByRole('dialog', { name: /tell an agent what to do/i })).toBeNull()
  })

  test('command palette opens Codex sign-in directly', async () => {
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.click(screen.getByTestId('top-bar-command-search'))
    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'codex login' },
    })

    await waitFor(() => {
      expect(
        screen.getByText('Open Codex sign-in before agents work on project files.')
      ).toBeDefined()
    })
    fireEvent.click(
      screen.getByText('Open Codex sign-in before agents work on project files.')
    )

    expect(onNavigate).toHaveBeenCalledWith('/settings/work-tool-sign-ins')
    expect(screen.queryByPlaceholderText(/search pages or things to do/i)).toBeNull()
  })

  test('command palette opens direct Settings sections for beginner setup searches', async () => {
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.click(screen.getByTestId('top-bar-command-search'))
    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'project settings' },
    })

    await waitFor(() => {
      expect(
        screen.getByText('Create or choose the project where tasks, agents, and files belong.')
      ).toBeDefined()
    })
    fireEvent.click(
      screen.getByText('Create or choose the project where tasks, agents, and files belong.')
    )

    expect(onNavigate).toHaveBeenCalledWith('/settings/projects')
    expect(screen.queryByPlaceholderText(/search pages or things to do/i)).toBeNull()
  })

  test('command palette restores and opens the setup checklist directly', async () => {
    const onNavigate = vi.fn()
    const setGettingStartedDismissed = vi.fn().mockResolvedValue(true)
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
      setGettingStartedDismissed,
    })

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.click(screen.getByTestId('top-bar-command-search'))
    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'start tutorial' },
    })

    await waitFor(() => {
      expect(
        screen.getByText(
          'Add the setup checklist back to the left menu and open it. Projects, agents, and tasks stay unchanged.'
        )
      ).toBeDefined()
    })
    fireEvent.click(screen.getByText('Show setup checklist'))

    await waitFor(() => expect(setGettingStartedDismissed).toHaveBeenCalledWith(false))
    await waitFor(() => expect(onNavigate).toHaveBeenCalledWith('/start'))
    expect(onNavigate).not.toHaveBeenCalledWith('/settings/account')
    expect(screen.queryByPlaceholderText(/search pages or things to do/i)).toBeNull()
  })

  test('command palette opens Account settings when setup checklist restore fails', async () => {
    const onNavigate = vi.fn()
    const setGettingStartedDismissed = vi.fn().mockResolvedValue(false)
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
      setGettingStartedDismissed,
    })

    render(<MemoryRouter onNavigate={onNavigate} />)

    fireEvent.click(screen.getByTestId('top-bar-command-search'))
    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'start tutorial' },
    })

    await waitFor(() => {
      expect(
        screen.getByText(
          'Add the setup checklist back to the left menu and open it. Projects, agents, and tasks stay unchanged.'
        )
      ).toBeDefined()
    })
    fireEvent.click(screen.getByText('Show setup checklist'))

    await waitFor(() => expect(setGettingStartedDismissed).toHaveBeenCalledWith(false))
    await waitFor(() => expect(onNavigate).toHaveBeenCalledWith('/settings/account'))
    expect(screen.queryByPlaceholderText(/search pages or things to do/i)).toBeNull()
  })

  test('uses beginner-facing start page metadata', () => {
    routerState.path = '/start'

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: 'Setup checklist' })).toBeDefined()
    expect(screen.getByText('Set up Forge and send your first task')).toBeDefined()
    expect(screen.queryByText(/^Start$/)).toBeNull()
    expect(screen.queryByText(/first-run setup/i)).toBeNull()
    expect(screen.queryByText(/launch checklist/i)).toBeNull()
  })

  test('uses beginner-facing tasks page metadata', () => {
    routerState.path = '/tasks'

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: 'Tasks' })).toBeDefined()
    expect(screen.getByText('Create tasks and follow agent progress')).toBeDefined()
    expect(screen.queryByText(/assign/i)).toBeNull()
    expect(screen.queryByText(/track agent work/i)).toBeNull()
  })

  test('uses beginner-facing inbox page metadata', () => {
    routerState.path = '/inbox'

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: 'Inbox' })).toBeDefined()
    expect(screen.getByText('Check updates that need a next step')).toBeDefined()
    expect(screen.queryByText('See what needs your attention')).toBeNull()
    expect(screen.queryByText(/notifications and updates/i)).toBeNull()
  })

  test('uses beginner-facing settings page metadata', () => {
    routerState.path = '/settings'

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: 'Settings' })).toBeDefined()
    expect(screen.getByText('Set up your account, AI services, and team')).toBeDefined()
    expect(screen.queryByText(/Account, AI services, and workspace/i)).toBeNull()
    expect(screen.queryByText(/model services/i)).toBeNull()
    expect(screen.queryByText(/providers/i)).toBeNull()
  })

  test('uses beginner-facing saved context page metadata', () => {
    routerState.path = '/context'

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: 'Saved notes and instructions' })).toBeDefined()
    expect(screen.getByText('Check what agents may reuse later')).toBeDefined()
    expect(screen.queryByText(/Saved\s+memories/i)).toBeNull()
    expect(screen.queryByText('Saved guidance')).toBeNull()
    expect(screen.queryByText(/approval queue/i)).toBeNull()
    expect(screen.queryByText(/governed context/i)).toBeNull()
  })

  test('uses beginner-facing agents page metadata', () => {
    routerState.path = '/agents'

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: 'Agents' })).toBeDefined()
    expect(screen.getByText('Create and manage agents that handle tasks')).toBeDefined()
    expect(screen.queryByText(/deploy and manage/i)).toBeNull()
    expect(screen.queryByText(/AI coding agents/i)).toBeNull()
  })

  test('uses beginner-facing analytics page metadata', () => {
    routerState.path = '/analytics'

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: 'Analytics' })).toBeDefined()
    expect(screen.getByText('See agent activity and results')).toBeDefined()
    expect(screen.queryByText(/performance and activity metrics/i)).toBeNull()
  })

  test('uses beginner-facing billing page metadata', () => {
    routerState.path = '/billing'

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: 'Billing' })).toBeDefined()
    expect(screen.getByText('Plan, payments, and invoices')).toBeDefined()
    expect(screen.queryByText(/usage/i)).toBeNull()
  })

  test('uses beginner-facing admin page metadata', () => {
    routerState.path = '/admin'

    render(<MemoryRouter />)

    expect(screen.getByRole('heading', { name: 'Admin' })).toBeDefined()
    expect(screen.getByText('Check app health and manage people')).toBeDefined()
    expect(screen.queryByText(/System health and user management/i)).toBeNull()
  })

  test('uses plain saved item history metadata', () => {
    routerState.path = '/context/audit'

    render(<MemoryRouter />)

    expect(screen.getByText('Saved item history')).toBeDefined()
    expect(screen.getByText('See what was checked or reused')).toBeDefined()
    expect(screen.queryByText(/exports/i)).toBeNull()
    expect(screen.queryByText(/governance event/i)).toBeNull()
  })

  test('does not expose task queue creation from the Tasks top bar', async () => {
    seedProjectNavigation('p1')
    useNavigationStore.setState({ agentGroups: [] })

    render(<MemoryRouter />)

    expect(screen.queryByRole('button', { name: /new task queue/i })).toBeNull()
    expect(screen.getByRole('combobox', { name: /where new tasks wait/i })).toBeDisabled()
    expect(mockCreateGroup).not.toHaveBeenCalled()
  })

  test('mobile keeps selected task detail accessible as an overlay', () => {
    Object.defineProperty(window, 'innerWidth', { value: 390, configurable: true })
    useBoardStore.getState().setTasks([
      {
        id: 'mobile-task',
        state: 'working',
        method: 'tasks/send',
        params: { task: 'Mobile task detail', message: 'Visible on mobile' },
        priority: 'normal',
        progress: 20,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])
    useBoardStore.getState().setSelectedTask('mobile-task')

    render(<MemoryRouter />)

    expect(screen.getByTestId('right-panel')).toBeDefined()
    expect(screen.getByText('Mobile task detail')).toBeDefined()
  })

  test('adds a task returned from the New Task modal to the board store', async () => {
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')
    const taskDetails =
      'Where to work:\n- src/app/features/board\n\nDone when:\n- AppLayout test passes'

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /new task/i }))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Modal task' },
    })
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: { value: taskDetails },
    })
    const modal = screen.getByRole('dialog')
    const [, prioritySelect, assigneeSelect] = within(modal).getAllByRole('combobox')
    fireEvent.change(prioritySelect, { target: { value: 'high' } })
    fireEvent.change(assigneeSelect, { target: { value: 'agent-1' } })
    fireEvent.click(screen.getByRole('button', { name: /create task/i }))

    await waitFor(() =>
      expect(mockCreateTask).toHaveBeenCalledWith({
        groupId: 'group-1',
        params: { task: 'Modal task', message: taskDetails },
        priority: 'high',
        assignedTo: 'agent-1',
      })
    )
    await waitFor(() => {
      expect(useBoardStore.getState().columns.backlog.map((task) => task.id)).toContain(
        'modal-task-1'
      )
    })
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  test('shows beginner guidance when New Task does not return a created task', async () => {
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')
    mockCreateTask.mockResolvedValueOnce({ ok: true, task: null })

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /new task/i }))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Modal task without result' },
    })
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value:
          'Where to work:\n- src/app/features/board\n\nDone when:\n- no task result returns guidance',
      },
    })
    fireEvent.click(screen.getByRole('button', { name: /create task/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'Check the project, where tasks wait, and the result, then create the task again. The task was not created.'
    )
    expect(alert.textContent).not.toContain('API')
  })

  test('routes no-agent setup from New Task to Agents', async () => {
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')
    mockGetParticipants.mockResolvedValueOnce([])
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)
    fireEvent.click(screen.getByRole('button', { name: /new task/i }))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    expect(screen.getByText('Connect an agent before this task can start')).toBeDefined()
    expect(screen.queryByText('No agents are online')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /open agents/i }))

    expect(onNavigate).toHaveBeenCalledWith('/agents')
    expect(screen.queryByRole('dialog', { name: /tell an agent what to do/i })).toBeNull()
  })

  test('routes unavailable-agent setup from New Task to Agents', async () => {
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')
    mockGetParticipants.mockResolvedValueOnce([
      { id: 'participant-1', agentId: 'agent-1', name: 'Busy Agent', status: 'busy' },
    ])
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)
    fireEvent.click(screen.getByRole('button', { name: /new task/i }))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    expect(screen.getByText('Start or connect an agent before this task can start')).toBeDefined()
    expect(screen.queryByText('No agents are available right now')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /open agents/i }))

    expect(onNavigate).toHaveBeenCalledWith('/agents')
    expect(screen.queryByRole('dialog', { name: /tell an agent what to do/i })).toBeNull()
  })

  test('asks for confirmation before creating from an unchanged task template', async () => {
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /new task/i }))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    const briefGroup = screen.getByRole('group', { name: /task templates/i })
    expect(screen.getByText(/clear task has three parts/i)).toBeDefined()
    expect(screen.getByText('What to finish')).toBeDefined()
    expect(screen.getByText(/visible change or decision/i)).toBeDefined()

    fireEvent.click(within(briefGroup).getByRole('button', { name: /fix a problem/i }))

    expect(screen.getByLabelText(/what should the agent finish/i)).toHaveValue(
      'Fix a problem you can repeat'
    )
    expect(screen.getByLabelText(/details the agent should know/i)).toHaveValue()
    expect(
      (screen.getByLabelText(/details the agent should know/i) as HTMLTextAreaElement).value
    ).toContain('What is broken:')

    fireEvent.click(screen.getByRole('button', { name: /create task/i }))
    await screen.findByTestId('task-brief-confirmation')
    expect(screen.getByText(/replace the template title/i)).toBeDefined()
    fireEvent.click(screen.getByRole('button', { name: /create task anyway/i }))

    await waitFor(() =>
      expect(mockCreateTask).toHaveBeenCalledWith({
        groupId: 'group-1',
        params: {
          task: 'Fix a problem you can repeat',
          message: expect.stringContaining('Done when:'),
        },
        priority: 'high',
      })
    )
  })

  test('lets New Task choose a project before creating', async () => {
    seedProjectNavigation(null)

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /new task/i }))

    const projectSelect = screen.getByLabelText(/project/i)
    const createButton = screen.getByRole('button', { name: /create task/i })
    expect(createButton).toBeEnabled()

    fireEvent.change(projectSelect, { target: { value: 'p1' } })
    await waitFor(() => expect(mockGetGroups).toHaveBeenCalledWith('p1'))

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Project-scoped task' },
    })
    await waitFor(() => expect(createButton).toBeEnabled())
    expect(screen.getByTestId('task-work-lane-readiness').textContent).toContain(
      'Task can be created'
    )
    fireEvent.click(createButton)
    await screen.findByTestId('task-brief-confirmation')
    fireEvent.click(screen.getByRole('button', { name: /create task anyway/i }))

    await waitFor(() =>
      expect(mockCreateTask).toHaveBeenCalledWith({
        groupId: 'group-1',
        params: { task: 'Project-scoped task', message: 'Project-scoped task' },
        priority: 'normal',
      })
    )
    expect(useNavigationStore.getState().selectedProjectId).toBe('p1')
  })

  test('requires a waiting place instead of initializing one from New Task', async () => {
    seedProjectNavigation(null)
    mockGetGroups.mockResolvedValue([])

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /new task/i }))
    const projectSelect = screen.getByLabelText(/project/i)
    const createButton = screen.getByRole('button', { name: /create task/i })
    expect(createButton).toBeEnabled()

    fireEvent.change(projectSelect, { target: { value: 'p1' } })
    await waitFor(() => expect(mockGetGroups).toHaveBeenCalledWith('p1'))

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Initialize project board' },
    })
    await waitFor(() =>
      expect(screen.getByTestId('task-work-lane-readiness').textContent).toContain(
        'Set up where tasks wait before creating this task'
      )
    )
    expect(screen.getByText(/Create one place for new work to wait/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /set up where tasks wait/i })).toBeDefined()
    const previousQueueInstruction = ['agents', 'check', 'task', 'queues'].join(' ')
    expect(screen.queryByText(new RegExp(previousQueueInstruction, 'i'))).toBeNull()
    expect(screen.queryByText(/task queue/i)).toBeNull()
    expect(createButton).toBeEnabled()
    fireEvent.click(createButton)
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Set up where tasks wait before saving this task.')
    expect(mockCreateGroup).not.toHaveBeenCalled()
    expect(mockCreateTask).not.toHaveBeenCalled()
    expect(useBoardStore.getState().selectedGroupId).toBeNull()
  }, 20_000)

  test('routes missing project setup from New Task to project settings', async () => {
    const onNavigate = vi.fn()
    render(<MemoryRouter onNavigate={onNavigate} />)
    fireEvent.click(screen.getByRole('button', { name: /set up project/i }))

    expect(onNavigate).toHaveBeenCalledWith('/settings/projects')
    expect(screen.queryByRole('dialog', { name: /tell an agent what to do/i })).toBeNull()
    expect(screen.queryByText(/no projects available/i)).toBeNull()
  })

  test('routes missing waiting place setup from New Task to Agents', async () => {
    seedProjectNavigation('p1')
    const onNavigate = vi.fn()

    render(<MemoryRouter onNavigate={onNavigate} />)
    fireEvent.click(screen.getByRole('button', { name: /set up waiting place/i }))

    expect(onNavigate).toHaveBeenCalledWith('/agents')
    expect(screen.queryByRole('dialog', { name: /tell an agent what to do/i })).toBeNull()
  })
})
