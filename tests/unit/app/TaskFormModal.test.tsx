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
  test('uses beginner-friendly brief template prompts', () => {
    render(
      <TaskFormModal
        isOpen
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        agents={[{ id: 'agent-1', name: 'Agent One', status: 'available' }]}
        projects={[project]}
        selectedProjectId={project.id}
        selectedTaskGroupId="lane-1"
        selectedTaskGroupName="Starter Queue"
      />
    )

    expect(screen.getByText('Start with a task template')).toBeDefined()
    expect(screen.getByText('Fills in a safe first draft')).toBeDefined()
    expect(screen.getByText(/what to include and how to check the work/i)).toBeDefined()
    expect(screen.getByRole('group', { name: /task templates/i })).toBeDefined()
    expect(screen.getByText('What to finish')).toBeDefined()
    expect(screen.getByText('Where to work')).toBeDefined()
    expect(screen.getByText('Done when')).toBeDefined()
    expect(screen.queryByText(/scope and proof/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /feature/i }))

    expect(screen.getByLabelText(/what should the agent finish/i)).toHaveValue(
      'Build a focused feature'
    )
    const description = screen.getByLabelText(
      /details the agent should know/i
    ) as HTMLTextAreaElement
    expect(description.value).toContain('What should change:')
    expect(description.value).toContain('Where to work:')
    expect(description.value).toContain('What to avoid:')
    expect(description.value).toContain('Done when:')
    expect(description.value).not.toContain('Scope:')
    expect(description.value).not.toContain('Constraints:')
    expect(description.value).not.toContain('Evidence:')
  })

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
        selectedTaskGroupName="Starter Queue"
      />
    )

    expect(
      screen.getByText(
        'No agents are online. You can create the task now; it will wait here until an agent comes online.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/dispatched?/i)).toBeNull()
  })

  test('guides busy-agent assignment without dispatch language', () => {
    render(
      <TaskFormModal
        isOpen
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        agents={[
          { id: 'agent-1', name: 'Busy Agent', status: 'busy' },
          { id: 'agent-2', name: 'Offline Agent', status: 'offline' },
        ]}
        projects={[project]}
        selectedProjectId={project.id}
        selectedTaskGroupId="lane-1"
        selectedTaskGroupName="Starter Queue"
      />
    )

    expect(
      screen.getByText(
        'No agents are available right now. Keep the default choice so the next available agent can pick it up.'
      )
    ).toBeDefined()
    expect(
      screen.getByRole('option', { name: /let the next available agent pick it up/i })
    ).toBeDefined()
    expect(screen.getByText(/any available agent can do the work/i)).toBeDefined()
    expect(screen.queryByText(/unassigned/i)).toBeNull()
    expect(screen.getByText(/people are waiting on it now/i)).toBeDefined()
    expect(screen.queryByText(/dispatch/i)).toBeNull()
  })

  test('explains a ready task queue without internal checking language', () => {
    render(
      <TaskFormModal
        isOpen
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        agents={[{ id: 'agent-1', name: 'Agent One', status: 'available' }]}
        projects={[project]}
        selectedProjectId={project.id}
        selectedTaskGroupId="lane-1"
        selectedTaskGroupName="Starter Queue"
      />
    )

    expect(screen.getByTestId('task-work-lane-readiness').textContent).toContain(
      'New tasks will wait in Starter Queue until an available agent picks them up.'
    )
    expect(
      screen.getByText(/Keep this choice when any available agent can do the work/i)
    ).toBeDefined()
    expect(screen.getByTestId('task-work-lane-readiness').textContent).not.toContain('is ready')
    const previousQueueInstruction = ['Agents', 'check', 'this', 'queue'].join(' ')
    expect(screen.getByTestId('task-work-lane-readiness').textContent).not.toContain(
      previousQueueInstruction
    )
    expect(screen.queryByText(/Leave this unassigned/i)).toBeNull()
  })

  test('explains task queue readiness before creating work', () => {
    const openTaskRouting = vi.fn()
    render(
      <TaskFormModal
        isOpen
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        agents={[{ id: 'agent-1', name: 'Agent One', status: 'available' }]}
        projects={[project]}
        selectedProjectId={project.id}
        selectedTaskGroupId={null}
        selectedTaskGroupName={null}
        onOpenTaskRouting={openTaskRouting}
      />
    )

    expect(screen.getByText('Create a Task Queue First')).toBeDefined()
    expect(screen.getByText(/A task queue gives new work a place to wait/i)).toBeDefined()
    expect(screen.getByTestId('task-work-lane-readiness').textContent).not.toContain(
      ['agent', 'is', 'ready'].join(' ')
    )

    fireEvent.click(screen.getByRole('button', { name: /open task queues/i }))

    expect(openTaskRouting).toHaveBeenCalled()
  })

  test('labels unknown agent states without exposing backend status values', () => {
    render(
      <TaskFormModal
        isOpen
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        agents={[
          { id: 'agent-1', name: 'Ready Agent', status: 'available' },
          { id: 'agent-2', name: 'Starting Agent', status: 'starting_up' },
          { id: 'agent-3', name: 'Missing Status Agent', status: ' ' },
        ]}
        projects={[project]}
        selectedProjectId={project.id}
        selectedTaskGroupId="lane-1"
        selectedTaskGroupName="Starter Queue"
      />
    )

    const readyOption = screen.getByRole('option', {
      name: 'Ready Agent (ready)',
    }) as HTMLOptionElement
    const unknownOption = screen.getByRole('option', {
      name: 'Starting Agent (not ready)',
    }) as HTMLOptionElement
    const missingStatusOption = screen.getByRole('option', {
      name: 'Missing Status Agent (status not reported)',
    }) as HTMLOptionElement

    expect(readyOption.disabled).toBe(false)
    expect(unknownOption.disabled).toBe(true)
    expect(missingStatusOption.disabled).toBe(true)
    expect(screen.queryByText(/starting_up/i)).toBeNull()
    expect(screen.queryByText(/Unknown/i)).toBeNull()
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
        selectedTaskGroupName="Starter Queue"
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
