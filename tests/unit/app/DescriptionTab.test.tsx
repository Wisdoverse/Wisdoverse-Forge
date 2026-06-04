import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { DescriptionTab } from '@app/features/detail/DescriptionTab'

afterEach(cleanup)

const mockTask = {
  id: 'task-1',
  groupId: 'g1',
  state: 'backlog' as const,
  method: 'tasks/send',
  params: { task: 'Review onboarding copy', message: 'Make the first run easier.' },
  priority: 'normal' as const,
  progress: 0,
  createdAt: new Date(Date.now() - 7200000).toISOString(),
  updatedAt: new Date().toISOString(),
}

describe('DescriptionTab', () => {
  test('explains when a backlog task still needs an agent', () => {
    render(<DescriptionTab task={mockTask} />)

    expect(screen.getByText('Needs agent')).toBeDefined()
    expect(screen.getByTestId('task-assignment-guidance').textContent).toBe(
      'Choose an agent before this task can leave the backlog.'
    )
  })

  test('does not call a task unassigned when only the agent id is loaded', () => {
    render(<DescriptionTab task={{ ...mockTask, assignedTo: 'agent-1' }} />)

    expect(screen.getByText('Assigned agent')).toBeDefined()
    expect(screen.getByTestId('task-assignment-guidance').textContent).toBe(
      'An agent is assigned, but its display name has not loaded yet.'
    )
    expect(screen.queryByText('Unassigned')).toBeNull()
  })

  test('explains queued tasks without internal runtime language', () => {
    render(<DescriptionTab task={{ ...mockTask, state: 'queued', assignedTo: 'agent-1' }} />)

    expect(screen.getByText('Waiting for the agent to start')).toBeDefined()
    expect(screen.getByTestId('task-next-action').textContent).toContain(
      'Keep the brief current while the assigned agent picks up the task.'
    )
    expect(screen.queryByText(/execution|runtime/i)).toBeNull()
  })

  test('keeps result and context actions available for completed tasks', () => {
    const onOpenResult = vi.fn()
    const onOpenContext = vi.fn()

    render(
      <DescriptionTab
        task={{
          ...mockTask,
          state: 'completed',
          assignedAgentName: 'Claude-2',
          result: [{ name: 'summary.md', mimeType: 'text/markdown', data: 'Done' }],
          contextCounts: { appliedMemories: 1, appliedSkills: 0, total: 1 },
        }}
        onOpenResult={onOpenResult}
        onOpenContext={onOpenContext}
      />
    )

    expect(screen.getByTestId('task-assignment-guidance').textContent).toBe(
      'This agent owns the next run for this task.'
    )

    fireEvent.click(screen.getByRole('button', { name: /open artifacts/i }))
    fireEvent.click(screen.getByRole('button', { name: /^review context/i }))

    expect(onOpenResult).toHaveBeenCalledOnce()
    expect(onOpenContext).toHaveBeenCalledOnce()
  })
})
