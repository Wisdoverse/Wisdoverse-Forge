import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { BoardToolbar, type BoardFilterCounts } from '@app/features/board/BoardToolbar'

afterEach(cleanup)

const counts: BoardFilterCounts = {
  total: 6,
  visible: 4,
  priority: {
    all: 6,
    urgent: 1,
    high: 2,
    normal: 2,
    low: 1,
  },
  assignee: {
    all: 6,
    assigned: 3,
    unassigned: 3,
  },
}

function renderToolbar(overrides: Partial<Parameters<typeof BoardToolbar>[0]> = {}) {
  const props = {
    searchQuery: '',
    onSearchQueryChange: vi.fn(),
    priorityFilter: 'all' as const,
    onPriorityFilterChange: vi.fn(),
    assigneeFilter: 'all' as const,
    onAssigneeFilterChange: vi.fn(),
    displayMode: 'comfortable' as const,
    onDisplayModeChange: vi.fn(),
    counts,
    onClear: vi.fn(),
    ...overrides,
  }

  render(<BoardToolbar {...props} />)
  return props
}

describe('BoardToolbar', () => {
  test('uses beginner-friendly filter labels and task counts', () => {
    renderToolbar()

    expect(
      screen.getByPlaceholderText('Search task names, agents, or help needed...')
    ).toBeDefined()
    expect(screen.getByRole('searchbox', { name: /search tasks/i })).toHaveAccessibleDescription(
      /use show all tasks to return to the full board/i
    )
    expect(screen.queryByPlaceholderText(/blockers/i)).toBeNull()
    expect(
      screen.getByRole('button', { name: /show tasks at all priority levels, 6 matching tasks/i })
    ).toBeDefined()
    expect(
      screen.getByRole('button', { name: /show tasks for all agent choices, 6 matching tasks/i })
    ).toBeDefined()
    expect(
      screen.getByRole('button', { name: /show tasks that still need an agent, 3 matching tasks/i })
    ).toBeDefined()
    expect(
      screen.getByRole('button', {
        name: /show tasks that already have an agent, 3 matching tasks/i,
      })
    ).toBeDefined()
    expect(screen.getByRole('status')).toHaveTextContent('Showing 4 of 6 tasks')
    expect(screen.getByRole('status')).toHaveAttribute('aria-live', 'polite')
  })

  test('keeps filter values stable while exposing clearer labels', () => {
    const props = renderToolbar()
    const toolbar = screen.getByTestId('board-toolbar')

    fireEvent.click(
      within(toolbar).getByRole('button', { name: /show urgent priority tasks, 1 matching task/i })
    )
    fireEvent.click(
      within(toolbar).getByRole('button', {
        name: /show tasks that still need an agent, 3 matching tasks/i,
      })
    )
    fireEvent.click(within(toolbar).getByRole('button', { name: /use compact task cards/i }))

    expect(props.onPriorityFilterChange).toHaveBeenCalledWith('urgent')
    expect(props.onAssigneeFilterChange).toHaveBeenCalledWith('unassigned')
    expect(props.onDisplayModeChange).toHaveBeenCalledWith('compact')
  })

  test('shows clear only when a search or filter is active', () => {
    const inactive = renderToolbar()
    expect(screen.queryByRole('button', { name: /show all tasks/i })).toBeNull()
    cleanup()

    const active = renderToolbar({ searchQuery: 'blocked' })

    fireEvent.click(screen.getByRole('button', { name: /show all tasks/i }))
    expect(active.onClear).toHaveBeenCalledOnce()
    expect(inactive.onClear).not.toHaveBeenCalled()
  })
})
