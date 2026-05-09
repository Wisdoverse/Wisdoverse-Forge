import { describe, test, expect, afterEach, beforeEach } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import { ListView } from '@app/features/list/ListView'
import { useBoardStore } from '@app/shared/model/board.store'

afterEach(cleanup)
beforeEach(() => useBoardStore.getState().reset())

describe('ListView', () => {
  test('renders table headers', () => {
    render(<ListView />)
    expect(screen.getByText('Title')).toBeDefined()
    expect(screen.getByText('Status')).toBeDefined()
    expect(screen.getByText('Assignee')).toBeDefined()
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
  })

  test('shows empty state when no tasks', () => {
    render(<ListView />)
    expect(screen.getByText(/no tasks/i)).toBeDefined()
  })
})
