import { describe, test, expect, afterEach, beforeEach } from 'vitest'
import { render, screen, cleanup, fireEvent, within } from '@testing-library/react'
import { ListView } from '@app/features/list/ListView'
import { useBoardStore } from '@app/shared/model/board.store'

afterEach(cleanup)
beforeEach(() => useBoardStore.getState().reset())

describe('ListView', () => {
  test('renders table headers', () => {
    render(<ListView />)
    expect(screen.getByText('Task result')).toBeDefined()
    expect(screen.queryByText('Title')).toBeNull()
    expect(screen.getByText('Status')).toBeDefined()
    expect(screen.getByText('Agent')).toBeDefined()
    expect(screen.queryByText('Assignee')).toBeNull()
    expect(screen.getByText('Priority')).toBeDefined()
  })

  test('renders task rows from all columns', () => {
    useBoardStore.getState().setTasks([
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
    ])
    render(<ListView />)
    expect(screen.getByText('Task A')).toBeDefined()
    expect(screen.getByText('Task B')).toBeDefined()
    expect(screen.getByText('Choose an agent or task queue, then send it.')).toBeDefined()
  })

  test('explains task agent fallbacks without placeholder symbols or raw ids', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'draft-without-agent',
        state: 'backlog',
        params: { task: 'Draft setup guide', message: '' },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
      {
        id: 'working-with-id',
        state: 'working',
        params: { task: 'Run smoke test', message: '' },
        assignedTo: 'agent-123',
        priority: 'normal',
        progress: 10,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
      {
        id: 'working-missing-agent',
        state: 'working',
        params: { task: 'Check deploy logs', message: '' },
        priority: 'normal',
        progress: 10,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    expect(screen.getByText('Choose where it runs')).toBeDefined()
    expect(screen.getByText('Assigned agent')).toBeDefined()
    expect(screen.getByText('Refresh tasks to load agent')).toBeDefined()
    expect(screen.queryByText('Agent not reported yet')).toBeNull()
    expect(screen.queryByText('agent-123')).toBeNull()
    expect(screen.queryByText('—')).toBeNull()
  })

  test('shows waiting tasks without queue wording', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'waiting-1',
        state: 'queued',
        params: { task: 'Prepare release notes', message: '' },
        assignedAgentName: 'Docs Agent',
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    expect(screen.getByText('Prepare release notes')).toBeDefined()
    expect(screen.getByText('Waiting to start')).toBeDefined()
    expect(screen.getByText(/available agent to start it/i)).toBeDefined()
    expect(screen.queryByText('Queued')).toBeNull()
    expect(screen.queryByText(/queue/i)).toBeNull()
  })

  test('shows empty state when no tasks', () => {
    useBoardStore.getState().setViewMode('list')

    render(<ListView />)
    expect(screen.getByTestId('list-empty-state')).toBeDefined()
    expect(screen.getByText('Create your first small task')).toBeDefined()
    expect(screen.getByText(/use the board to create one small task/i)).toBeDefined()
    expect(screen.getByText(/proof you expect the agent to return/i)).toBeDefined()
    expect(screen.queryByText(/Create one small task from the board first/i)).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /open board to create task/i }))
    expect(useBoardStore.getState().viewMode).toBe('board')
    expect(screen.queryByText('No tasks yet')).toBeNull()
  })

  test('summarizes task work register across lifecycle states', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'backlog-1',
        state: 'backlog',
        params: { task: 'Plan onboarding', message: '' },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
      {
        id: 'working-1',
        state: 'working',
        params: { task: 'Build settings', message: '' },
        assignedAgentName: 'Build Runner',
        priority: 'high',
        progress: 40,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
      {
        id: 'blocked-1',
        state: 'blocked',
        params: { task: 'Deploy preview', message: 'Waiting on approval' },
        priority: 'urgent',
        progress: 20,
        blockedHint: 'Waiting on approval',
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
      {
        id: 'done-1',
        state: 'completed',
        params: { task: 'Document setup', message: '' },
        priority: 'low',
        progress: 100,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    expect(screen.getByTestId('list-work-register')).toBeDefined()
    expect(screen.getByTestId('list-next-step')).toHaveTextContent(
      /Start with 1 task needing action/
    )
    expect(screen.getByText(/Open work that needs help or recovery first/i)).toBeDefined()
    expect(screen.queryByText(/Open the blocked or failed work first/i)).toBeNull()
    expect(within(screen.getByTestId('list-metric-active')).getByText('1')).toBeDefined()
    expect(within(screen.getByTestId('list-metric-backlog')).getByText('1')).toBeDefined()
    expect(
      within(screen.getByTestId('list-metric-backlog')).getByText('Not sent yet')
    ).toBeDefined()
    expect(within(screen.getByTestId('list-metric-attention')).getByText('1')).toBeDefined()
    expect(within(screen.getByTestId('list-metric-completed')).getByText('1')).toBeDefined()
    expect(screen.getByText(/Help needed: Waiting on approval/i)).toBeDefined()
    expect(screen.queryByText(/Resolve blocker/i)).toBeNull()
  })

  test('summarizes unsent tasks without backlog or lane wording', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'draft-1',
        state: 'backlog',
        params: { task: 'Draft onboarding checklist', message: '' },
        priority: 'normal',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    expect(screen.getByTestId('list-next-step')).toHaveTextContent(
      'Send 1 task after choosing where it should run.'
    )
    expect(screen.getByText(/Choose an agent or task queue, then send the work/i)).toBeDefined()
    expect(screen.getByTestId('list-next-step').textContent).not.toContain('when ready')
    expect(screen.getByTestId('list-work-register').textContent).not.toContain('backlog task')
    expect(screen.getByTestId('list-work-register').textContent).not.toContain('next lane')
  })

  test('guides completed task review without evidence jargon', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'done-1',
        state: 'completed',
        params: { task: 'Document setup', message: '' },
        priority: 'low',
        progress: 100,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    expect(screen.getByTestId('list-next-step')).toHaveTextContent('Review completed work.')
    expect(
      screen.getByText(
        'Open completed tasks to check the result, result files, and anything worth reusing.'
      )
    ).toBeDefined()
    expect(screen.getByText('Open it to review the result and result files.')).toBeDefined()
    expect(screen.queryByText(/result, evidence/i)).toBeNull()
    expect(screen.queryByText(/result and evidence/i)).toBeNull()
  })

  test('filters task list by attention state and search', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'working-1',
        state: 'working',
        params: { task: 'Build settings', message: '' },
        assignedAgentName: 'Build Runner',
        priority: 'high',
        progress: 40,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
      {
        id: 'blocked-1',
        state: 'blocked',
        params: { task: 'Deploy preview', message: 'Waiting on approval' },
        assignedAgentName: 'Release Agent',
        priority: 'urgent',
        progress: 20,
        blockedHint: 'Waiting on approval',
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    fireEvent.click(
      within(screen.getByTestId('list-task-filter')).getByRole('button', {
        name: /needs action\s*1/i,
      })
    )

    expect(screen.getByText('Deploy preview')).toBeDefined()
    expect(screen.queryByText('Build settings')).toBeNull()

    fireEvent.change(screen.getByTestId('list-search'), {
      target: { value: 'missing task' },
    })
    const combinedEmpty = screen.getByTestId('list-filter-empty')
    expect(within(combinedEmpty).getByText('Clear search or show all tasks')).toBeDefined()
    expect(combinedEmpty.textContent).toContain(
      'There are tasks here, but the current search and filter hide them.'
    )

    expect(screen.queryByText(/narrow by task result/i)).toBeNull()
    expect(screen.queryByText(/No tasks match this view/i)).toBeNull()
    expect(screen.queryByText(/task title/i)).toBeNull()
    expect(screen.queryByText(/blocker/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /show all tasks/i }))
    expect(screen.getByText('Build settings')).toBeDefined()
  })

  test('explains search-only and filter-only empty task lists', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'working-1',
        state: 'working',
        params: { task: 'Build settings', message: '' },
        assignedAgentName: 'Build Runner',
        priority: 'high',
        progress: 40,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    fireEvent.change(screen.getByTestId('list-search'), {
      target: { value: 'missing task' },
    })
    const searchEmpty = screen.getByTestId('list-filter-empty')
    expect(within(searchEmpty).getByText('Clear search to see tasks')).toBeDefined()
    expect(searchEmpty.textContent).toContain(
      'There are tasks here, but this search hides them. Try a broader word.'
    )
    expect(searchEmpty.textContent).not.toContain('No tasks match this view')

    fireEvent.click(screen.getByRole('button', { name: /show all tasks/i }))
    fireEvent.click(
      within(screen.getByTestId('list-task-filter')).getByRole('button', {
        name: /completed\s*0/i,
      })
    )

    const filterEmpty = screen.getByTestId('list-filter-empty')
    expect(within(filterEmpty).getByText('Choose All to see tasks')).toBeDefined()
    expect(filterEmpty.textContent).toContain(
      'There are tasks here, but this filter has no results yet.'
    )
    expect(filterEmpty.textContent).not.toContain('No tasks match this view')
  })

  test('shows blocked reason guidance without raw reason codes', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'blocked-raw',
        state: 'blocked',
        params: { task: 'Scale preview', message: '' },
        priority: 'urgent',
        progress: 20,
        blockedReason: 'quota_exceeded',
        error: 'quota_exceeded: docker socket denied secret token abc',
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    expect(
      screen.getByText(/Help needed: Pause lower-priority work or ask an owner/i)
    ).toBeDefined()
    expect(screen.queryByText(/Resolve blocker/i)).toBeNull()
    expect(screen.queryByText(/quota_exceeded/i)).toBeNull()
    expect(screen.queryByText(/docker socket/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
  })

  test('shows failed task recovery guidance without failure jargon', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'failed-raw',
        state: 'failed',
        params: { task: 'Recover deploy', message: '' },
        priority: 'urgent',
        progress: 90,
        error: 'HTTP 500 provider token stack trace',
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    expect(
      screen.getByText(
        'Open it, review the recovery note, then retry only after the next step is clear.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/read the failure/i)).toBeNull()
    expect(screen.queryByText(/fix the error/i)).toBeNull()
    expect(screen.queryByText(/HTTP 500/i)).toBeNull()
    expect(screen.queryByText(/provider token/i)).toBeNull()
  })

  test('labels unknown row status and priority without exposing raw codes', () => {
    useBoardStore.getState().setTasks([
      {
        id: 'unknown-state',
        state: 'waiting_for_agent',
        params: { task: 'Review release gate', message: '' },
        priority: 'future_priority',
        progress: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      } as any,
    ])

    render(<ListView />)

    expect(screen.getByText('Check task status')).toBeDefined()
    expect(screen.getByText('Check task priority')).toBeDefined()
    expect(screen.queryByText(/waiting_for_agent/i)).toBeNull()
    expect(screen.queryByText(/waiting for agent/i)).toBeNull()
    expect(screen.queryByText(/future_priority/i)).toBeNull()
    expect(screen.queryByText(/future priority/i)).toBeNull()
  })
})
