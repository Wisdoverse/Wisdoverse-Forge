import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react'
import { useBoardStore } from '@app/shared/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'

const mockGetParticipants = vi.fn()
const mockCreateTask = vi.fn()
const mockGetGroups = vi.fn()
const mockCreateGroup = vi.fn()

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
    select({ location: { pathname: '/tasks' } }),
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

    expect(screen.getByPlaceholderText('Search commands...')).toBeDefined()
  })

  test('does not expose task group creation from the Tasks top bar', async () => {
    seedProjectNavigation('p1')
    useNavigationStore.setState({ agentGroups: [] })

    render(<MemoryRouter />)

    expect(screen.queryByRole('button', { name: /new task group/i })).toBeNull()
    expect(screen.getByRole('combobox', { name: /work lane for new tasks/i })).toBeDisabled()
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
    fireEvent.change(screen.getByPlaceholderText(/what needs to be done/i), {
      target: { value: 'Modal task' },
    })
    fireEvent.change(screen.getByPlaceholderText(/additional details/i), {
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

  test('applies a brief template before creating a New Task', async () => {
    seedProjectNavigation('p1')
    useBoardStore.getState().setSelectedGroupId('group-1')

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /\+ task/i }))

    await waitFor(() => expect(mockGetParticipants).toHaveBeenCalledWith('all'))
    const briefGroup = screen.getByRole('group', { name: /task brief templates/i })
    fireEvent.click(within(briefGroup).getByRole('button', { name: /bug/i }))

    expect(screen.getByPlaceholderText(/what needs to be done/i)).toHaveValue(
      'Fix a reproducible defect'
    )
    expect(screen.getByPlaceholderText(/additional details/i)).toHaveValue()
    expect(
      (screen.getByPlaceholderText(/additional details/i) as HTMLTextAreaElement).value
    ).toContain('Symptom:')

    fireEvent.click(screen.getByRole('button', { name: /create task/i }))

    await waitFor(() =>
      expect(mockCreateTask).toHaveBeenCalledWith({
        groupId: 'group-1',
        params: {
          task: 'Fix a reproducible defect',
          message: expect.stringContaining('Verification:'),
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

    fireEvent.change(screen.getByPlaceholderText(/what needs to be done/i), {
      target: { value: 'Project-scoped task' },
    })
    await waitFor(() => expect(createButton).toBeEnabled())
    expect(screen.getByTestId('task-work-lane-readiness').textContent).toContain('Work Lane Ready')
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

  test('requires a task group instead of initializing one from New Task', async () => {
    seedProjectNavigation(null)
    mockGetGroups.mockResolvedValue([])

    render(<MemoryRouter />)
    fireEvent.click(screen.getByRole('button', { name: /\+ task/i }))
    const projectSelect = screen.getByLabelText(/project/i)
    const createButton = screen.getByRole('button', { name: /create task/i })
    expect(createButton).toBeDisabled()

    fireEvent.change(projectSelect, { target: { value: 'p1' } })
    await waitFor(() => expect(mockGetGroups).toHaveBeenCalledWith('p1'))

    fireEvent.change(screen.getByPlaceholderText(/what needs to be done/i), {
      target: { value: 'Initialize project board' },
    })
    await waitFor(() =>
      expect(screen.getByTestId('task-work-lane-readiness').textContent).toContain(
        'Create a Work Lane First'
      )
    )
    expect(screen.getByText(/agents listen to work lanes/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /open task routing/i })).toBeDefined()
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
