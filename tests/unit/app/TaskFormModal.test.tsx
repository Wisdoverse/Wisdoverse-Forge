import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { TaskFormModal, type TaskProjectOption } from '@app/features/board/TaskFormModal'

afterEach(cleanup)

const project: TaskProjectOption = {
  id: 'project-1',
  name: 'Starter Project',
  teamId: 'team-1',
  teamName: 'Starter Team',
}

describe('TaskFormModal', () => {
  test('explains the no-agent state without dispatch language', () => {
    render(
      <TaskFormModal
        isOpen
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        agents={[]}
        projects={[project]}
        selectedProjectId={project.id}
        selectedTaskGroupId="lane-1"
        selectedTaskGroupName="Starter Lane"
      />
    )

    expect(
      screen.getByText(
        'No online agents available. New tasks will wait here until an agent comes online.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/dispatched?/i)).toBeNull()
  })

  test('shows a beginner-safe title error before submitting', async () => {
    const onSubmit = vi.fn()

    render(
      <TaskFormModal
        isOpen
        onClose={vi.fn()}
        onSubmit={onSubmit}
        agents={[{ id: 'agent-1', name: 'Agent One', status: 'available' }]}
        projects={[project]}
        selectedProjectId={project.id}
        selectedTaskGroupId="lane-1"
        selectedTaskGroupName="Starter Lane"
      />
    )

    fireEvent.click(screen.getByRole('button', { name: /create task/i }))

    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(
        'Add a short title so the agent knows the goal.'
      )
    })
    expect(onSubmit).not.toHaveBeenCalled()
  })
})
