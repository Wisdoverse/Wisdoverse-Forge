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
    expect(screen.queryByPlaceholderText(/blockers/i)).toBeNull()
    expect(screen.getByRole('button', { name: /all priorities\s*6/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /all agents\s*6/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /needs agent\s*3/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /has agent\s*3/i })).toBeDefined()
    expect(screen.getByText('Showing 4 of 6 tasks')).toBeDefined()
  })

  test('keeps filter values stable while exposing clearer labels', () => {
    const props = renderToolbar()
    const toolbar = screen.getByTestId('board-toolbar')

    fireEvent.click(within(toolbar).getByRole('button', { name: /urgent\s*1/i }))
    fireEvent.click(within(toolbar).getByRole('button', { name: /needs agent\s*3/i }))
    fireEvent.click(within(toolbar).getByRole('button', { name: /compact/i }))

    expect(props.onPriorityFilterChange).toHaveBeenCalledWith('urgent')
    expect(props.onAssigneeFilterChange).toHaveBeenCalledWith('unassigned')
    expect(props.onDisplayModeChange).toHaveBeenCalledWith('compact')
  })

  test('shows clear only when a search or filter is active', () => {
    const inactive = renderToolbar()
    expect(screen.queryByRole('button', { name: 'Clear' })).toBeNull()
    cleanup()

    const active = renderToolbar({ searchQuery: 'blocked' })

    fireEvent.click(screen.getByRole('button', { name: 'Clear' }))
    expect(active.onClear).toHaveBeenCalledOnce()
    expect(inactive.onClear).not.toHaveBeenCalled()
  })
})
