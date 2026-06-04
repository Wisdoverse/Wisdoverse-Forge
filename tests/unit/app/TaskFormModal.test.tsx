import { cleanup, render, screen } from '@testing-library/react'
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
})
