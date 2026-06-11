import { describe, test, expect, afterEach, vi, beforeEach } from 'vitest'
import { render, screen, cleanup, waitFor, act, fireEvent, within } from '@testing-library/react'
import { BoardView } from '@app/features/board/BoardView'
import { useBoardStore } from '@app/shared/model/board.store'
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
  mockCreateTask.mockClear()
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
  test('shows no-group placeholder when no group is selected', () => {
    render(<BoardView />)
    expect(screen.getByTestId('board-no-group')).toBeDefined()
    expect(screen.getByText(/pick a project to start/i)).toBeDefined()
  })

  test('explains missing task queue when a project is selected', () => {
    useNavigationStore.setState({ selectedProjectId: 'p1' })

    render(<BoardView />)

    expect(screen.getByText(/set up a task queue first/i)).toBeDefined()
    expect(screen.getByText(/a task queue is where new tasks wait/i)).toBeDefined()
    expect(screen.getByText(/agents > work lanes/i)).toBeDefined()
  })

  test('renders task lifecycle columns with correct headers', async () => {
    useBoardStore.getState().setSelectedGroupId('test-group')
    render(<BoardView />)
    await waitFor(() => {
      expect(screen.getAllByText('Backlog').length).toBeGreaterThan(0)
    })
    expect(screen.getByText('Waiting to start')).toBeDefined()
    expect(screen.getByText('Working')).toBeDefined()
    expect(screen.getAllByText('Blocked').length).toBeGreaterThan(0)
    expect(screen.getByText('Done')).toBeDefined()
    expect(screen.getByText('Needs review')).toBeDefined()
    expect(screen.queryByText('Failed')).toBeNull()
    expect(screen.getByText('Canceled')).toBeDefined()
  })

  test('shows beginner sign-in guidance when tasks fail to load', async () => {
    mockGetTasks.mockRejectedValueOnce(new Error('401 Unauthorized'))
    useBoardStore.getState().setSelectedGroupId('test-group')

    render(<BoardView />)

    const error = await screen.findByTestId('board-error')
    expect(error.textContent).toContain('Sign in again')
    expect(error.textContent).not.toContain('Code:')
    expect(error.textContent).not.toContain('401 Unauthorized')
  })

  test('shows beginner network guidance when readiness cannot load', async () => {
    mockGetParticipants.mockRejectedValueOnce(new TypeError('Failed to fetch'))
    useBoardStore.getState().setSelectedGroupId('test-group')

    render(<BoardView />)

    expect(
      await screen.findByText(/forge could not connect while loading the board/i)
    ).toBeDefined()
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
    expect(screen.getAllByText('Available').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Busy').length).toBeGreaterThan(0)
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
        params: { task: 'Blocked task', message: '' },
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

    expect(await screen.findByText(/2 unassigned tasks can be handed off/i)).toBeDefined()
    expect(screen.getByTestId('assignment-metric-backlog').textContent).toContain('2')
    expect(screen.getByTestId('assignment-metric-unassigned').textContent).toContain('2')
    expect(screen.getByTestId('assignment-metric-in-flight').textContent).toContain('1')
    expect(screen.getByTestId('assignment-metric-blocked').textContent).toContain('1')
    expect(screen.getByTestId('assignment-metric-review').textContent).toContain('1')
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
    expect(screen.getByTestId('board-filter-empty')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /clear filters/i }))
    expect(screen.getByText('API migration')).toBeDefined()
    expect(screen.getByText('Dashboard polish')).toBeDefined()
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

    fireEvent.click(within(toolbar).getByRole('button', { name: /urgent\s*1/i }))
    expect(screen.getByText('Production incident')).toBeDefined()
    expect(screen.queryByText('Copy review')).toBeNull()

    fireEvent.click(within(toolbar).getByRole('button', { name: /all priorities\s*2/i }))
    fireEvent.click(within(toolbar).getByRole('button', { name: /^has agent\s*1$/i }))
    expect(screen.queryByText('Production incident')).toBeNull()
    expect(screen.getByText('Copy review')).toBeDefined()
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
