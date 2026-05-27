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
  test('explains empty backlog as the draft task starting point', () => {
    renderColumn('backlog')

    const emptyState = screen.getByTestId('kanban-empty-backlog')
    expect(within(emptyState).getByText('No draft tasks')).toBeDefined()
    expect(within(emptyState).getByText(/quick add below/i)).toBeDefined()
  })

  test('explains empty lifecycle columns by their next visible state', () => {
    renderColumn('working')

    const emptyState = screen.getByTestId('kanban-empty-working')
    expect(within(emptyState).getByText('No active runs')).toBeDefined()
    expect(within(emptyState).getByText(/once an agent starts the task/i)).toBeDefined()
  })
})
