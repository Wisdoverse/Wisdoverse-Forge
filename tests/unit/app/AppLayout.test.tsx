import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react'
import { useBoardStore } from '@app/shared/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'

const mockGetParticipants = vi.fn()
const mockCreateTask = vi.fn()
const mockGetGroups = vi.fn()
const mockCreateGroup = vi.fn()
const routerState = vi.hoisted(() => ({ path: '/tasks' }))

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

afterEach(() => {
  cleanup()
  useBoardStore.getState().reset()
  useNavigationStore.getState().reset()
  vi.clearAllMocks()
  Object.defineProperty(window, 'innerWidth', { value: 1024, configurable: true })
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
    // Right panel defaults to collapsed — assert the reveal affordance instead
    const revealButton = screen.getByRole('button', { name: /show activity panel/i })
    expect(revealButton).toBeDefined()
    expect(within(revealButton).getByText('Activity')).toBeDefined()
  })

  test('sidebar has navigation items', () => {
    render(<MemoryRouter />)
    const navItems = screen.getAllByTestId(/^sidebar-nav-/)
    expect(navItems.length).toBeGreaterThanOrEqual(1)
    expect(screen.getByTestId('sidebar-nav-start')).toBeDefined()
  })

  test('top bar shows view toggles', () => {
    render(<MemoryRouter />)
    expect(screen.getByText('Board')).toBeDefined()
    expect(screen.getByText('List')).toBeDefined()
    expect(screen.getByText('Timeline')).toBeDefined()
    expect(screen.getByText('3D')).toBeDefined()
  })

  test('top bar labels the command search entry for beginners', () => {
    render(<MemoryRouter />)

    const searchButton = screen.getByTestId('top-bar-command-search')
    expect(searchButton).toHaveAccessibleName('Search commands and pages')
    expect(screen.getByText('Search')).toBeDefined()

    fireEvent.click(searchButton)

    expect(screen.getByPlaceholderText(/search commands/i)).toBeDefined()
  })

  test('uses beginner-facing start page metadata', () => {
    routerState.path = '/start'

    render(<MemoryRouter />)

    expect(screen.getByText('Start')).toBeDefined()
    expect(screen.getByText('Set up Forge and send your first task')).toBeDefined()
    expect(screen.queryByText(/first-run setup/i)).toBeNull()
    expect(screen.queryByText(/launch checklist/i)).toBeNull()
  })

  test('uses beginner-facing settings page metadata', () => {
    routerState.path = '/settings'

    render(<MemoryRouter />)

    expect(screen.getByText('Settings')).toBeDefined()
    expect(screen.getByText('Account, AI services, and workspace')).toBeDefined()
    expect(screen.queryByText(/model services/i)).toBeNull()
    expect(screen.queryByText(/providers/i)).toBeNull()
  })

  test('uses beginner-facing saved context page metadata', () => {
    routerState.path = '/context'

    render(<MemoryRouter />)

    expect(screen.getByText('Saved memories and instructions')).toBeDefined()
    expect(screen.getByText('Review what agents may reuse later')).toBeDefined()
    expect(screen.queryByText('Saved guidance')).toBeNull()
    expect(screen.queryByText(/approval queue/i)).toBeNull()
    expect(screen.queryByText(/governed context/i)).toBeNull()
  })

  test('uses beginner-facing agents page metadata', () => {
    routerState.path = '/agents'

    render(<MemoryRouter />)

    expect(screen.getByText('Agents')).toBeDefined()
    expect(screen.getByText('Create and manage agents that handle tasks')).toBeDefined()
    expect(screen.queryByText(/deploy and manage/i)).toBeNull()
    expect(screen.queryByText(/AI coding agents/i)).toBeNull()
  })

  test('uses beginner-facing analytics page metadata', () => {
    routerState.path = '/analytics'

    render(<MemoryRouter />)

    expect(screen.getByText('Analytics')).toBeDefined()
    expect(screen.getByText('See agent activity and results')).toBeDefined()
    expect(screen.queryByText(/performance and activity metrics/i)).toBeNull()
  })

  test('uses beginner-facing billing page metadata', () => {
    routerState.path = '/billing'

    render(<MemoryRouter />)

    expect(screen.getByText('Billing')).toBeDefined()
    expect(screen.getByText('Plan, payments, and invoices')).toBeDefined()
    expect(screen.queryByText(/usage/i)).toBeNull()
  })

  test('uses plain review history metadata', () => {
    routerState.path = '/context/audit'

    render(<MemoryRouter />)

    expect(screen.getByText('Review history')).toBeDefined()
    expect(screen.getByText('See what was reviewed or reused')).toBeDefined()
    expect(screen.queryByText(/exports/i)).toBeNull()
    expect(screen.queryByText(/governance event/i)).toBeNull()
  })

  test('does not expose task queue creation from the Tasks top bar', async () => {
    seedProjectNavigation('p1')
    useNavigationStore.setState({ agentGroups: [] })

    render(<MemoryRouter />)

    expect(screen.queryByRole('button', { name: /new task queue/i })).toBeNull()
    expect(screen.getByRole('combobox', { name: /task queue for new tasks/i })).toBeDisabled()
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

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /\+ task/i }))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Modal task' },
    })
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: { value: 'Details' },
    })
    const modal = screen.getByRole('dialog')
    const [, prioritySelect, assigneeSelect] = within(modal).getAllByRole('combobox')
    fireEvent.change(prioritySelect, { target: { value: 'high' } })
    fireEvent.change(assigneeSelect, { target: { value: 'agent-1' } })
    fireEvent.click(screen.getByRole('button', { name: /create task/i }))

    await waitFor(() =>
      expect(mockCreateTask).toHaveBeenCalledWith({
        groupId: 'group-1',
        params: { task: 'Modal task', message: 'Details' },
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
    fireEvent.click(screen.getByRole('button', { name: /\+ task/i }))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Modal task without result' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create task/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'The task was not created. Check the project, task queue, and result, then try again.'
    )
    expect(alert.textContent).not.toContain('API')
  })

  test('applies a task template before creating a New Task', async () => {
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /\+ task/i }))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    const briefGroup = screen.getByRole('group', { name: /task templates/i })
    expect(screen.getByText(/clear task has three parts/i)).toBeDefined()
    expect(screen.getByText('What to finish')).toBeDefined()
    expect(screen.getByText(/visible change or decision/i)).toBeDefined()

    fireEvent.click(within(briefGroup).getByRole('button', { name: /bug/i }))

    expect(screen.getByLabelText(/what should the agent finish/i)).toHaveValue(
      'Fix a reproducible defect'
    )
    expect(screen.getByLabelText(/details the agent should know/i)).toHaveValue()
    expect(
      (screen.getByLabelText(/details the agent should know/i) as HTMLTextAreaElement).value
    ).toContain('What is broken:')

    fireEvent.click(screen.getByRole('button', { name: /create task/i }))

    await waitFor(() =>
      expect(mockCreateTask).toHaveBeenCalledWith({
        groupId: 'group-1',
        params: {
          task: 'Fix a reproducible defect',
          message: expect.stringContaining('Done when:'),
        },
        priority: 'high',
      })
    )
  })

  test('lets New Task choose a project before creating', async () => {
    seedProjectNavigation(null)

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /\+ task/i }))

    const projectSelect = screen.getByLabelText(/project/i)
    const createButton = screen.getByRole('button', { name: /create task/i })
    expect(createButton).toBeDisabled()

    fireEvent.change(projectSelect, { target: { value: 'p1' } })
    await waitFor(() => expect(mockGetGroups).toHaveBeenCalledWith('p1'))

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Project-scoped task' },
    })
    await waitFor(() => expect(createButton).toBeEnabled())
    expect(screen.getByTestId('task-work-lane-readiness').textContent).toContain('Ready to Send')
    fireEvent.click(createButton)

    await waitFor(() =>
      expect(mockCreateTask).toHaveBeenCalledWith({
        groupId: 'group-1',
        params: { task: 'Project-scoped task', message: 'Project-scoped task' },
        priority: 'normal',
      })
    )
    expect(useNavigationStore.getState().selectedProjectId).toBe('p1')
  })

  test('requires a task queue instead of initializing one from New Task', async () => {
    seedProjectNavigation(null)
    mockGetGroups.mockResolvedValue([])

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /\+ task/i }))
    const projectSelect = screen.getByLabelText(/project/i)
    const createButton = screen.getByRole('button', { name: /create task/i })
    expect(createButton).toBeDisabled()

    fireEvent.change(projectSelect, { target: { value: 'p1' } })
    await waitFor(() => expect(mockGetGroups).toHaveBeenCalledWith('p1'))

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Initialize project board' },
    })
    await waitFor(() =>
      expect(screen.getByTestId('task-work-lane-readiness').textContent).toContain(
        'Create a Task Queue First'
      )
    )
    expect(screen.getByText(/a task queue gives new work a place to wait/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /open task queues/i })).toBeDefined()
    expect(createButton).toBeDisabled()
    expect(mockCreateGroup).not.toHaveBeenCalled()
    expect(mockCreateTask).not.toHaveBeenCalled()
    expect(useBoardStore.getState().selectedGroupId).toBeNull()
  }, 20_000)

  test('disables New Task submission when there are no projects', () => {
    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /\+ task/i }))

    expect(screen.getByText(/no projects available/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /create task/i })).toBeDisabled()
  })
})
