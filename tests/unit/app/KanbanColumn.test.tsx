import { DndContext } from '@dnd-kit/core'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { KanbanColumn } from '@app/features/board/KanbanColumn'
import type { TaskSummary } from '@app/shared/api/orchestration'

afterEach(cleanup)

function renderColumn(columnId: string, tasks: TaskSummary[] = [], onQuickCreate = vi.fn()) {
  const view = render(
    <DndContext>
      <KanbanColumn columnId={columnId} tasks={tasks} onQuickCreate={onQuickCreate} />
    </DndContext>
  )
  return { ...view, onQuickCreate }
}

describe('KanbanColumn', () => {
  test('explains empty unsent tasks as the task starting point', () => {
    renderColumn('backlog')

    expect(screen.getByText('Not sent yet')).toBeDefined()
    expect(screen.queryByText('Backlog')).toBeNull()
    const emptyState = screen.getByTestId('kanban-empty-backlog')
    expect(within(emptyState).getByText('Add the first task below')).toBeDefined()
    expect(within(emptyState).getByText(/add a task below with the result you want/i)).toBeDefined()
    expect(within(emptyState).queryByText('No tasks waiting to send')).toBeNull()
    expect(emptyState.textContent).not.toMatch(/quick add/i)
    expect(emptyState.textContent).not.toMatch(/draft task/i)
  })

  test('offers plain task examples in quick create', async () => {
    const { onQuickCreate } = renderColumn('backlog')

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))

    expect(screen.getByText('Need a starting point?')).toBeDefined()
    const examples = screen.getByRole('group', { name: /task examples/i })
    fireEvent.click(
      within(examples).getByRole('button', { name: /Check setup and list the next safe step/i })
    )

    expect(screen.getByLabelText('Task goal')).toHaveValue(
      'Check setup and list the next safe step'
    )
    expect(
      within(examples).queryByRole('button', { name: /Review setup and list the next safe step/i })
    ).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /save for later/i }))

    await waitFor(() =>
      expect(onQuickCreate).toHaveBeenCalledWith(
        'Check setup and list the next safe step',
        'backlog'
      )
    )
  })

  test('explains empty lifecycle columns by their next visible state', () => {
    renderColumn('working')

    const emptyState = screen.getByTestId('kanban-empty-working')
    expect(within(emptyState).getByText('Start a waiting task to show live work')).toBeDefined()
    expect(
      within(emptyState).getByText(/choose an agent, then choose Preview and send/i)
    ).toBeDefined()
    expect(within(emptyState).queryByText(/choose Start/i)).toBeNull()
    expect(within(emptyState).queryByText('No work in progress')).toBeNull()
    expect(within(emptyState).queryByText('No active runs')).toBeNull()
    expect(within(emptyState).queryByText('Running work appears here')).toBeNull()
  })

  test('explains waiting tasks without queue or dispatch language', () => {
    renderColumn('queued')

    const emptyState = screen.getByTestId('kanban-empty-queued')
    expect(within(emptyState).getByText('Send a task to an agent first')).toBeDefined()
    expect(within(emptyState).getByText(/choose an agent and send a task/i)).toBeDefined()
    expect(within(emptyState).queryByText('Nothing waiting to start')).toBeNull()
    expect(emptyState.textContent).not.toMatch(/queue|queued/i)
    expect(emptyState.textContent).not.toContain('dispatch')
  })

  test('explains empty help-needed tasks without blocker jargon', () => {
    renderColumn('blocked')

    expect(screen.getByText('Needs help')).toBeDefined()
    expect(screen.queryByText('Blocked')).toBeNull()
    const emptyState = screen.getByTestId('kanban-empty-blocked')
    expect(within(emptyState).getByText('Answer help requests from this column')).toBeDefined()
    expect(within(emptyState).getByText(/read what the agent needs/i)).toBeDefined()
    expect(within(emptyState).getByText(/choose Allow and continue/i)).toBeDefined()
    expect(within(emptyState).queryByText(/Continue or Stop/i)).toBeNull()
    expect(within(emptyState).queryByText('Nothing needs help')).toBeNull()
    expect(emptyState.textContent).not.toMatch(/blocker|owner input/i)
    expect(within(emptyState).queryByText('Tasks needing your answer appear here')).toBeNull()
  })

  test('explains review and recovery columns by when work appears there', () => {
    renderColumn('done')

    const reviewEmpty = screen.getByTestId('kanban-empty-done')
    expect(within(reviewEmpty).getByText('Check finished work before using it')).toBeDefined()
    expect(
      within(reviewEmpty).getByText(/save repeatable steps or create a follow-up/i)
    ).toBeDefined()
    expect(within(reviewEmpty).queryByText('Nothing ready for review')).toBeNull()
    expect(within(reviewEmpty).queryByText('Finished work appears here for review')).toBeNull()

    cleanup()
    renderColumn('failed')

    const recoveryEmpty = screen.getByTestId('kanban-empty-failed')
    expect(within(recoveryEmpty).getByText('Retry stopped work from this column')).toBeDefined()
    expect(within(recoveryEmpty).getByText(/read the recovery note/i)).toBeDefined()
    expect(recoveryEmpty.textContent).not.toMatch(/retry path/i)
    expect(within(recoveryEmpty).queryByText('No work needing recovery')).toBeNull()
  })
})
