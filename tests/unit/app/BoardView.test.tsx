import { describe, test, expect, afterEach, vi, beforeEach } from 'vitest'
import { render, screen, cleanup, waitFor, act, fireEvent, within } from '@testing-library/react'
import { BoardView } from '@app/features/board/BoardView'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'

const boardSocketMocks = vi.hoisted(() => ({
  status: 'disconnected' as 'connecting' | 'connected' | 'disconnected',
}))
const mockGetTasks = vi.fn().mockResolvedValue([])
const mockCreateTask = vi.fn().mockResolvedValue({ ok: true, task: null })
const mockUpdateTask = vi.fn().mockResolvedValue({ ok: true })
const mockGetParticipants = vi.fn().mockResolvedValue([])

vi.mock('@app/shared/model/websocket.context', () => ({
  useWebSocket: () => ({
    status: boardSocketMocks.status,
    send: vi.fn(),
    subscribe: vi.fn(() => () => {}),
  }),
}))

vi.mock('@app/shared/api/orchestration', () => ({
  taskResultArtifacts: (result: unknown) => (Array.isArray(result) ? result : []),
  orchestrationApi: {
    getTasks: (...args: unknown[]) => mockGetTasks(...args),
    createTask: (...args: unknown[]) => mockCreateTask(...args),
    updateTask: (...args: unknown[]) => mockUpdateTask(...args),
    getParticipants: (...args: unknown[]) => mockGetParticipants(...args),
  },
}))

beforeEach(() => {
  boardSocketMocks.status = 'disconnected'
  mockGetTasks.mockClear().mockResolvedValue([])
  mockCreateTask.mockClear().mockResolvedValue({ ok: true, task: null })
  mockUpdateTask.mockClear()
  mockGetParticipants.mockClear().mockResolvedValue([])
})

afterEach(() => {
  vi.useRealTimers()
  cleanup()
  useBoardStore.getState().reset()
  useNavigationStore.getState().reset()
})

describe('BoardView', () => {
  test('explains the main task board loading state for first-time users', () => {
    mockGetTasks.mockImplementationOnce(() => new Promise(() => undefined))
    useBoardStore.getState().setSelectedGroupId('test-group')
    useBoardStore.setState({ loading: true })

    render(<BoardView />)

    const loading = screen.getByRole('status', { name: /checking tasks/i })
    expect(loading).toHaveTextContent('Checking tasks')
    expect(loading).toHaveTextContent(
      'Forge is checking which tasks are waiting, working, need help, or finished in this project.'
    )
    expect(loading).toHaveTextContent(
      'If this takes more than a moment, open Tasks again or ask an owner or admin to check the place for new tasks.'
    )
    expect(loading).toHaveTextContent(
      'Success looks like task columns or an add-the-first-task step.'
    )
    expect(loading).not.toHaveTextContent('Loading tasks')
  })

  test('shows no-group placeholder when no group is selected', () => {
    const onOpenProjectsSetup = vi.fn()

    render(<BoardView onOpenProjectsSetup={onOpenProjectsSetup} />)

    expect(screen.getByTestId('board-no-group')).toBeDefined()
    expect(screen.getByText(/create or choose a project before creating tasks/i)).toBeDefined()
    expect(screen.getByText(/open project settings to create a project/i)).toBeDefined()
    expect(screen.queryByText(/choose a project from the sidebar/i)).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /open project settings/i }))
    expect(onOpenProjectsSetup).toHaveBeenCalledTimes(1)
  })

  test('explains missing place for new tasks when a project is selected', () => {
    useNavigationStore.setState({ selectedProjectId: 'p1' })
    const onOpenTaskQueues = vi.fn()

    render(<BoardView onOpenTaskQueues={onOpenTaskQueues} />)

    expect(screen.getByText(/set up a place for new tasks before sending work/i)).toBeDefined()
    expect(screen.getByText(/new tasks need a place before an agent starts them/i)).toBeDefined()
    expect(screen.queryByText(/set up where tasks wait before sending work/i)).toBeNull()
    expect(screen.queryByText(/open task queues to create one/i)).toBeNull()
    expect(screen.queryByText(/task queue/i)).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /set up place/i }))
    expect(onOpenTaskQueues).toHaveBeenCalledTimes(1)
  })

  test('renders task lifecycle columns with correct headers', async () => {
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)
    await waitFor(() => {
      expect(screen.getAllByText('Not sent yet').length).toBeGreaterThan(0)
    })
    expect(screen.queryByText('Backlog')).toBeNull()
    expect(screen.getByText('Waiting to start')).toBeDefined()
    expect(screen.getByText('Working')).toBeDefined()
    expect(screen.getAllByText('Needs help').length).toBeGreaterThan(0)
    expect(screen.queryByText('Blocked')).toBeNull()
    expect(screen.getByText('Done')).toBeDefined()
    expect(screen.getByText(/check the result, then save repeatable steps/i)).toBeDefined()
    expect(screen.getByText(/create a follow-up task/i)).toBeDefined()
    expect(screen.queryByText(/saved guidance/i)).toBeNull()
    expect(screen.getByText('Check retry steps')).toBeDefined()
    expect(screen.queryByText('Failed')).toBeNull()
    expect(screen.getByText('Canceled')).toBeDefined()
  })

  test('shows beginner sign-in guidance when tasks fail to load', async () => {
    mockGetTasks.mockRejectedValueOnce(new Error('401 Unauthorized'))
    useBoardStore.getState().setSelectedGroupId('test-group')

    render(<BoardView />)

    const error = await screen.findByTestId('board-error')
    const alert = within(error).getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(error.textContent).toContain('Sign in again')
    expect(error.textContent).not.toContain('Code:')
    expect(error.textContent).not.toContain('401 Unauthorized')
    expect(within(error).getByRole('button', { name: /check tasks again/i })).toBeDefined()
    expect(within(error).queryByRole('button', { name: /try again/i })).toBeNull()
  })

  test('lets users retry when tasks fail to load', async () => {
    mockGetTasks.mockRejectedValueOnce(new Error('HTTP 503')).mockResolvedValueOnce([])
    useBoardStore.getState().setSelectedGroupId('test-group')

    render(<BoardView />)

    const error = await screen.findByTestId('board-error')
    fireEvent.click(within(error).getByRole('button', { name: /check tasks again/i }))

    await waitFor(() => expect(mockGetTasks).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(screen.queryByTestId('board-error')).toBeNull())
    expect(screen.getByTestId('assignment-readiness')).toBeDefined()
  })

  test('shows beginner network guidance when readiness cannot load', async () => {
    mockGetParticipants.mockRejectedValueOnce(new TypeError('Failed to fetch'))
    useBoardStore.getState().setSelectedGroupId('test-group')

    render(<BoardView />)

    const readiness = await screen.findByTestId('assignment-readiness')
    expect(readiness.textContent).toContain('Choose Check agent status before sending work.')
    expect(readiness.textContent).toContain(
      'If it still does not load, check your connection, then choose Check agent status.'
    )
    expect(screen.queryByText(/failed to fetch/i)).toBeNull()
  })

  test('shows board-level assignment readiness with agent blockers', async () => {
    mockGetParticipants.mockResolvedValueOnce([
      {
        id: 'participant-1',
        agentId: 'agent-1',
        name: 'Ready Agent',
        status: 'available',
        capabilities: ['codex'],
      },
      {
        id: 'participant-2',
        agentId: 'agent-2',
        name: 'Busy Agent',
        status: 'busy',
        capabilities: ['claude'],
      },
    ])
    useBoardStore.getState().setSelectedGroupId('test-group')

    render(<BoardView />)

    expect(await screen.findByTestId('assignment-readiness')).toBeDefined()
    expect(screen.getByText(/1 agent can take work now/i)).toBeDefined()
    expect(screen.getByText('Ready Agent')).toBeDefined()
    expect(screen.getByText('Busy Agent')).toBeDefined()
    expect(screen.getAllByText('Can take work').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Working now').length).toBeGreaterThan(0)
  })

  test('summarizes work handoff pressure from board columns', async () => {
    mockGetParticipants.mockResolvedValueOnce([
      {
        id: 'participant-1',
        agentId: 'agent-1',
        name: 'Ready Agent',
        status: 'available',
        capabilities: ['codex'],
      },
    ])
    mockGetTasks.mockResolvedValueOnce([
      {
        id: 'backlog-1',
        state: 'backlog',
        params: { task: 'Unassigned task A', message: '' },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
      {
        id: 'backlog-2',
        state: 'backlog',
        params: { task: 'Unassigned task B', message: '' },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
      {
        id: 'working-1',
        state: 'working',
        params: { task: 'Running task', message: '' },
        assignedTo: 'agent-1',
        assignedAgentName: 'Ready Agent',
        priority: 'normal',
        progress: 50,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
      {
        id: 'blocked-1',
        state: 'blocked',
        params: { task: 'Task waiting for help', message: '' },
        assignedTo: 'agent-1',
        assignedAgentName: 'Ready Agent',
        priority: 'high',
        progress: 10,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
      {
        id: 'done-1',
        state: 'completed',
        params: { task: 'Completed task', message: '' },
        assignedTo: 'agent-1',
        assignedAgentName: 'Ready Agent',
        priority: 'normal',
        progress: 100,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        completedAt: new Date().toISOString(),
      },
    ] as any)
    useBoardStore.getState().setSelectedGroupId('test-group')

    render(<BoardView />)

    expect(await screen.findByText(/2 tasks need an agent/i)).toBeDefined()
    expect(screen.getByText(/choose an available agent to start them/i)).toBeDefined()
    expect(screen.getByTestId('assignment-metric-backlog').textContent).toContain('2')
    expect(screen.getByTestId('assignment-metric-unassigned').textContent).toContain('2')
    expect(screen.getByTestId('assignment-metric-unassigned').textContent).toContain('Needs agent')
    expect(screen.getByTestId('assignment-metric-working').textContent).toContain('1')
    expect(screen.getByTestId('assignment-metric-working').textContent).toContain('Being worked on')
    expect(screen.queryByTestId('assignment-metric-in-flight')).toBeNull()
    expect(screen.getByTestId('assignment-metric-blocked').textContent).toContain('1')
    expect(screen.getByTestId('assignment-metric-blocked').textContent).toContain('Needs help')
    expect(screen.getByTestId('assignment-metric-ready-to-check').textContent).toContain('1')
    expect(screen.getByTestId('assignment-metric-ready-to-check').textContent).toContain(
      'Ready to check'
    )
    expect(screen.queryByTestId('assignment-metric-review')).toBeNull()
  })

  test('renders failed tasks outside the Done column', async () => {
    mockGetTasks.mockResolvedValueOnce([
      {
        id: 'failed-task',
        state: 'failed',
        params: { task: 'Task failed with auth error', message: '' },
        priority: 'normal',
        progress: 0,
        error: '401 Unauthorized',
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
    ] as any)
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)
    await waitFor(() => {
      expect(screen.getByText('Task failed with auth error')).toBeDefined()
    })
    expect(screen.getByTestId('column-count-done').textContent).toBe('0')
    expect(screen.getByTestId('column-count-failed').textContent).toBe('1')
  })

  test('renders task cards in correct columns', async () => {
    const tasks = [
      {
        id: '1',
        state: 'backlog',
        params: { task: 'Task A', message: '' },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
      {
        id: '2',
        state: 'working',
        params: { task: 'Task B', message: '' },
        priority: 'high',
        progress: 50,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ]
    mockGetTasks.mockResolvedValueOnce(tasks)
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)
    await waitFor(() => {
      expect(screen.getByText('Task A')).toBeDefined()
    })
    expect(screen.getByText('Task B')).toBeDefined()
  })

  test('filters board cards by task search and clears empty results', async () => {
    mockGetTasks.mockResolvedValueOnce([
      {
        id: 'api-1',
        state: 'backlog',
        params: { task: 'API migration', message: 'Move settings to Rust' },
        priority: 'urgent',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
      {
        id: 'ui-1',
        state: 'working',
        params: { task: 'Dashboard polish', message: 'Tighten board cards' },
        assignedTo: 'agent-1',
        assignedAgentName: 'Design Agent',
        priority: 'low',
        progress: 50,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
    ] as any)
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)

    expect(await screen.findByText('API migration')).toBeDefined()
    fireEvent.change(screen.getByTestId('board-search'), { target: { value: 'dashboard' } })
    expect(screen.queryByText('API migration')).toBeNull()
    expect(screen.getByText('Dashboard polish')).toBeDefined()

    fireEvent.change(screen.getByTestId('board-search'), { target: { value: 'missing' } })
    const emptyState = screen.getByTestId('board-filter-empty')
    expect(emptyState).toHaveAttribute('role', 'status')
    expect(emptyState).toHaveAttribute('aria-live', 'polite')
    expect(within(emptyState).getByText('Search is hiding tasks')).toBeDefined()
    expect(
      within(emptyState).getByText(/show all tasks, then search with fewer words/i)
    ).toBeDefined()
    expect(within(emptyState).getByText(/before deciding the board is empty/i)).toBeDefined()
    expect(emptyState.textContent).not.toContain('No tasks match your search')
    expect(emptyState.textContent).not.toContain('No Tasks Match This Board View')
    expect(emptyState.textContent).not.toContain('full workflow')

    fireEvent.click(within(emptyState).getByRole('button', { name: /show all tasks/i }))
    expect(screen.getByText('API migration')).toBeDefined()
    expect(screen.getByText('Dashboard polish')).toBeDefined()
  })

  test('does not match hidden task ids in board search', async () => {
    mockGetTasks.mockResolvedValueOnce([
      {
        id: 'internal-ticket-42',
        state: 'backlog',
        params: { task: 'Write customer handoff note', message: 'Summarize the next step' },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
    ] as any)
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)

    expect(await screen.findByText('Write customer handoff note')).toBeDefined()
    fireEvent.change(screen.getByTestId('board-search'), {
      target: { value: 'internal-ticket-42' },
    })

    const emptyState = screen.getByTestId('board-filter-empty')
    expect(emptyState).toHaveTextContent('Search is hiding tasks')
    expect(screen.queryByText('Write customer handoff note')).toBeNull()
  })

  test('does not match hidden task descriptions in board search', async () => {
    mockGetTasks.mockResolvedValueOnce([
      {
        id: 'brief-hidden-1',
        state: 'backlog',
        params: {
          task: 'Prepare customer summary',
          message: 'internal-only rollout migration note',
        },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
    ] as any)
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)

    expect(await screen.findByText('Prepare customer summary')).toBeDefined()
    expect(screen.queryByText('internal-only rollout migration note')).toBeNull()
    fireEvent.change(screen.getByTestId('board-search'), {
      target: { value: 'internal-only rollout migration note' },
    })

    const emptyState = screen.getByTestId('board-filter-empty')
    expect(emptyState).toHaveTextContent('Search is hiding tasks')
    expect(screen.queryByText('Prepare customer summary')).toBeNull()
  })

  test('explains empty board choices without filter jargon', async () => {
    mockGetTasks.mockResolvedValueOnce([
      {
        id: 'assigned-1',
        state: 'queued',
        params: { task: 'Review launch note', message: '' },
        assignedTo: 'agent-1',
        assignedAgentName: 'Reviewer',
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
    ] as any)
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)

    expect(await screen.findByText('Review launch note')).toBeDefined()
    const toolbar = screen.getByTestId('board-toolbar')
    fireEvent.click(within(toolbar).getByRole('button', { name: /^filters$/i }))

    fireEvent.click(
      within(toolbar).getByRole('button', { name: /show high priority tasks, 0 matching tasks/i })
    )
    const priorityEmpty = screen.getByTestId('board-filter-empty')
    expect(priorityEmpty).toHaveTextContent('Priority choice is hiding tasks')
    expect(priorityEmpty).toHaveTextContent(
      'Tasks may still exist. Show all tasks, then choose one priority at a time.'
    )
    expect(priorityEmpty).toHaveTextContent(
      'Next: show all tasks before deciding this priority is empty.'
    )
    expect(priorityEmpty).not.toHaveTextContent('No tasks match this priority')
    expect(priorityEmpty).not.toHaveTextContent('priority filter')

    fireEvent.click(within(priorityEmpty).getByRole('button', { name: /show all tasks/i }))
    fireEvent.click(
      within(toolbar).getByRole('button', {
        name: /show tasks that still need an agent, 0 matching tasks/i,
      })
    )
    const agentEmpty = screen.getByTestId('board-filter-empty')
    expect(agentEmpty).toHaveTextContent('Agent choice is hiding tasks')
    expect(agentEmpty).toHaveTextContent(
      'Tasks may still exist. Show all tasks, then choose one agent option at a time.'
    )
    expect(agentEmpty).toHaveTextContent('Next: show all tasks before deciding nothing is waiting.')
    expect(agentEmpty).not.toHaveTextContent('No tasks match this agent choice')
    expect(agentEmpty).not.toHaveTextContent('agent filter')

    fireEvent.change(screen.getByTestId('board-search'), { target: { value: 'missing' } })
    const combinedEmpty = screen.getByTestId('board-filter-empty')
    expect(combinedEmpty).toHaveTextContent('Search and choices are hiding tasks')
    expect(combinedEmpty).toHaveTextContent(
      'The board still has tasks. Show all tasks, then narrow one choice at a time.'
    )
    expect(combinedEmpty).not.toHaveTextContent('No tasks match this view')
    expect(combinedEmpty).not.toHaveTextContent('Filters are hiding every task')
  })

  test('filters board cards by priority and assignee state', async () => {
    mockGetTasks.mockResolvedValueOnce([
      {
        id: 'urgent-1',
        state: 'backlog',
        params: { task: 'Production incident', message: '' },
        priority: 'urgent',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
      {
        id: 'low-1',
        state: 'queued',
        params: { task: 'Copy review', message: '' },
        assignedTo: 'agent-1',
        assignedAgentName: 'Reviewer',
        priority: 'low',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
    ] as any)
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)

    expect(await screen.findByText('Production incident')).toBeDefined()
    const toolbar = screen.getByTestId('board-toolbar')

    fireEvent.click(within(toolbar).getByRole('button', { name: /^filters$/i }))
    fireEvent.click(
      within(toolbar).getByRole('button', { name: /show urgent priority tasks, 1 matching task/i })
    )
    expect(screen.getByText('Production incident')).toBeDefined()
    expect(screen.queryByText('Copy review')).toBeNull()

    fireEvent.click(
      within(toolbar).getByRole('button', {
        name: /show tasks at all priority levels, 2 matching tasks/i,
      })
    )
    fireEvent.click(
      within(toolbar).getByRole('button', {
        name: /show tasks that already have an agent, 1 matching task/i,
      })
    )
    expect(screen.queryByText('Production incident')).toBeNull()
    expect(screen.getByText('Copy review')).toBeDefined()
  })

  test('shows beginner guidance when quick task creation returns no task', async () => {
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)

    fireEvent.click(await screen.findByRole('button', { name: /add task idea/i }))
    fireEvent.change(screen.getByLabelText(/task goal/i), {
      target: { value: 'Task without result' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save for later$/i }))

    const alert = await screen.findByTestId('board-action-error')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Add the task result, choose a project and a place for new tasks, then create the task again. The task was not created.'
    )
    expect(screen.getByLabelText(/task goal/i)).toHaveAccessibleDescription(
      /add the task result, choose a project and a place for new tasks, then create the task again/i
    )
    expect(alert.textContent).not.toContain('task queue')
    expect(alert.textContent).not.toContain('API')
  })

  test('shows column task count', async () => {
    const tasks = [
      {
        id: '1',
        state: 'working',
        params: { task: 'Task A', message: '' },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
      {
        id: '2',
        state: 'working',
        params: { task: 'Task B', message: '' },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ]
    mockGetTasks.mockResolvedValueOnce(tasks)
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)
    await waitFor(() => {
      expect(screen.getByTestId('column-count-working').textContent).toBe('2')
    })
  })

  test('polls selected group as websocket fallback', async () => {
    vi.useFakeTimers()
    useBoardStore.getState().setSelectedGroupId('test-group')

    render(<BoardView />)
    await act(async () => {
      await Promise.resolve()
    })
    expect(mockGetTasks).toHaveBeenCalledTimes(1)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000)
    })

    expect(mockGetTasks).toHaveBeenCalledTimes(2)
  })

  test('skips fallback refresh while live updates are connected', async () => {
    vi.useFakeTimers()
    boardSocketMocks.status = 'connected'
    useBoardStore.getState().setSelectedGroupId('test-group')

    render(<BoardView />)
    await act(async () => {
      await Promise.resolve()
    })
    expect(mockGetTasks).toHaveBeenCalledTimes(1)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000)
    })

    expect(mockGetTasks).toHaveBeenCalledTimes(1)
  })
})
