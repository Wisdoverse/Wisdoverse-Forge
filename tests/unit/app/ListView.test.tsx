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
    render(<ListView />)
    expect(screen.getByTestId('list-empty-state')).toBeDefined()
    expect(screen.getByText(/create one small task/i)).toBeDefined()
    expect(screen.getByText(/proof you expect the agent to return/i)).toBeDefined()
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
    expect(screen.getByText(/Open the blocked or failed work first/i)).toBeDefined()
    expect(within(screen.getByTestId('list-metric-active')).getByText('1')).toBeDefined()
    expect(within(screen.getByTestId('list-metric-backlog')).getByText('1')).toBeDefined()
    expect(
      within(screen.getByTestId('list-metric-backlog')).getByText('Not sent yet')
    ).toBeDefined()
    expect(within(screen.getByTestId('list-metric-attention')).getByText('1')).toBeDefined()
    expect(within(screen.getByTestId('list-metric-completed')).getByText('1')).toBeDefined()
    expect(screen.getByText(/Resolve blocker: Waiting on approval/i)).toBeDefined()
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
    expect(screen.getByTestId('list-filter-empty')).toBeDefined()

    expect(screen.getByText(/narrow by task result/i)).toBeDefined()
    expect(screen.queryByText(/task title/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /show all tasks/i }))
    expect(screen.getByText('Build settings')).toBeDefined()
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
      screen.getByText(/Resolve blocker: Pause lower-priority work or ask an owner/i)
    ).toBeDefined()
    expect(screen.queryByText(/quota_exceeded/i)).toBeNull()
    expect(screen.queryByText(/docker socket/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
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

    expect(screen.getByText('Status needs review')).toBeDefined()
    expect(screen.getByText('Priority needs review')).toBeDefined()
    expect(screen.queryByText(/waiting_for_agent/i)).toBeNull()
    expect(screen.queryByText(/waiting for agent/i)).toBeNull()
    expect(screen.queryByText(/future_priority/i)).toBeNull()
    expect(screen.queryByText(/future priority/i)).toBeNull()
  })
})
