import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { RecurringTasksBlock } from '@app/pages/settings/ui/RecurringTasksBlock'
import { useNavigationStore } from '@app/entities/navigation'

const listRecurringTasksMock = vi.hoisted(() => vi.fn())
const createRecurringTaskMock = vi.hoisted(() => vi.fn())
const updateRecurringTaskMock = vi.hoisted(() => vi.fn())
const deleteRecurringTaskMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/orchestration', () => ({
  orchestrationApi: {
    listRecurringTasks: listRecurringTasksMock,
    createRecurringTask: createRecurringTaskMock,
    updateRecurringTask: updateRecurringTaskMock,
    deleteRecurringTask: deleteRecurringTaskMock,
  },
}))

const getGroupsMock = vi.hoisted(() => vi.fn())

vi.mock('@app/entities/navigation/agent-group', () => ({
  agentGroupApi: { getGroups: getGroupsMock },
}))

beforeEach(() => {
  useNavigationStore.getState().reset()
  listRecurringTasksMock.mockReset()
  createRecurringTaskMock.mockReset()
  updateRecurringTaskMock.mockReset()
  deleteRecurringTaskMock.mockReset()
  getGroupsMock.mockReset()
  listRecurringTasksMock.mockResolvedValue([])
  getGroupsMock.mockResolvedValue([{ id: 'group-1', name: 'Inbox', projectId: 'project-1' }])
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('RecurringTasksBlock', () => {
  test('lists schedules and pauses with one click', async () => {
    listRecurringTasksMock.mockResolvedValue([
      {
        id: 'r-1',
        name: 'Daily summary',
        title: 'Summarize yesterday',
        description: '',
        priority: 'normal',
        requiresApproval: false,
        projectId: 'project-1',
        groupId: 'group-1',
        cadenceMinutes: 1440,
        nextRunAt: new Date().toISOString(),
        enabled: true,
        createdAt: new Date().toISOString(),
      },
    ])
    updateRecurringTaskMock.mockResolvedValue({
      id: 'r-1',
      name: 'Daily summary',
      title: 'Summarize yesterday',
      description: '',
      priority: 'normal',
      requiresApproval: false,
      projectId: 'project-1',
      groupId: 'group-1',
      cadenceMinutes: 1440,
      nextRunAt: new Date().toISOString(),
      enabled: false,
      createdAt: new Date().toISOString(),
    })
    render(<RecurringTasksBlock />)

    expect(await screen.findByText('Daily summary')).toBeDefined()
    expect(screen.getAllByText('Every day').length).toBeGreaterThan(0)
    fireEvent.click(screen.getByTestId('toggle-recurring-task-r-1'))
    await waitFor(() => expect(updateRecurringTaskMock).toHaveBeenCalledWith('r-1', false))
    expect((await screen.findByTestId('recurring-task-r-1')).textContent).toContain('paused')
  })

  test('creates a schedule from project, place and cadence', async () => {
    useNavigationStore.setState({
      projects: {
        teamId: [{ id: 'project-1', name: 'Website', teamId: 'team-1', teamName: 'Team' }],
      },
    })
    createRecurringTaskMock.mockResolvedValue({
      id: 'r-2',
      name: 'Weekly cleanup',
      title: 'Clean up test data',
      description: '',
      priority: 'normal',
      requiresApproval: false,
      projectId: 'project-1',
      groupId: 'group-1',
      cadenceMinutes: 10080,
      nextRunAt: new Date().toISOString(),
      enabled: true,
      createdAt: new Date().toISOString(),
    })
    render(<RecurringTasksBlock />)
    await screen.findByText(/No recurring tasks yet/)

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Weekly cleanup' } })
    fireEvent.change(screen.getByLabelText('Task title each run uses'), {
      target: { value: 'Clean up test data' },
    })
    fireEvent.change(screen.getByLabelText('Repeat'), { target: { value: '10080' } })
    fireEvent.change(screen.getByLabelText('Project'), { target: { value: 'project-1' } })
    await waitFor(() => expect(getGroupsMock).toHaveBeenCalledWith('project-1'))
    await screen.findByRole('option', { name: 'Inbox' })
    fireEvent.change(screen.getByLabelText('Place for new tasks'), { target: { value: 'group-1' } })
    fireEvent.click(screen.getByTestId('save-recurring-task'))

    await waitFor(() => expect(createRecurringTaskMock).toHaveBeenCalledTimes(1))
    expect(createRecurringTaskMock).toHaveBeenCalledWith({
      name: 'Weekly cleanup',
      title: 'Clean up test data',
      projectId: 'project-1',
      groupId: 'group-1',
      cadenceMinutes: 10080,
      requiresApproval: false,
    })
    expect(await screen.findByText('Weekly cleanup')).toBeDefined()
  })
})
