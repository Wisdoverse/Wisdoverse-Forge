import { DndContext } from '@dnd-kit/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { KanbanColumn } from '@app/features/board/KanbanColumn'
import type { TaskSummary } from '@app/shared/api/orchestration'

afterEach(cleanup)

function renderColumn(columnId: string, tasks: TaskSummary[] = []) {
  return render(
    <DndContext>
      <KanbanColumn columnId={columnId} tasks={tasks} onQuickCreate={vi.fn()} />
    </DndContext>
  )
}

describe('KanbanColumn', () => {
  test('explains empty unsent tasks as the task starting point', () => {
    renderColumn('backlog')

    expect(screen.getByText('Not sent yet')).toBeDefined()
    expect(screen.queryByText('Backlog')).toBeNull()
    const emptyState = screen.getByTestId('kanban-empty-backlog')
    expect(within(emptyState).getByText('No tasks waiting to send')).toBeDefined()
    expect(within(emptyState).getByText(/add a task below with the result you want/i)).toBeDefined()
    expect(emptyState.textContent).not.toMatch(/quick add/i)
    expect(emptyState.textContent).not.toMatch(/draft task/i)
  })

  test('explains empty lifecycle columns by their next visible state', () => {
    renderColumn('working')

    const emptyState = screen.getByTestId('kanban-empty-working')
    expect(within(emptyState).getByText('No work in progress')).toBeDefined()
    expect(within(emptyState).getByText(/once an agent starts the task/i)).toBeDefined()
    expect(within(emptyState).queryByText('No active runs')).toBeNull()
  })

  test('explains waiting tasks without queue or dispatch language', () => {
    renderColumn('queued')

    const emptyState = screen.getByTestId('kanban-empty-queued')
    expect(within(emptyState).getByText('Nothing waiting to start')).toBeDefined()
    expect(within(emptyState).getByText(/available agent starts them/i)).toBeDefined()
    expect(emptyState.textContent).not.toMatch(/queue|queued/i)
    expect(emptyState.textContent).not.toContain('dispatch')
  })

  test('explains empty help-needed tasks without blocker jargon', () => {
    renderColumn('blocked')

    expect(screen.getByText('Needs help')).toBeDefined()
    expect(screen.queryByText('Blocked')).toBeNull()
    const emptyState = screen.getByTestId('kanban-empty-blocked')
    expect(within(emptyState).getByText('Nothing needs help')).toBeDefined()
    expect(
      within(emptyState).getByText(/waiting for your answer or missing details/i)
    ).toBeDefined()
    expect(emptyState.textContent).not.toMatch(/blocker|owner input/i)
  })
})
