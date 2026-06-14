import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { TaskFormModal, type TaskProjectOption } from '@app/features/board/TaskFormModal'

const project: TaskProjectOption = {
  id: 'project-1',
  name: 'Starter Project',
  teamId: 'team-1',
  teamName: 'Starter Team',
}

const otherProject: TaskProjectOption = {
  id: 'project-2',
  name: 'Other Project',
  teamId: 'team-1',
  teamName: 'Starter Team',
}

function renderModal(
  onSubmit = vi.fn(),
  overrides: Partial<{
    agents: { id: string; name: string; status: string }[]
    projects: TaskProjectOption[]
    selectedProjectId: string | null
    selectedTaskGroupId: string | null
    selectedTaskGroupName: string | null
    onProjectChange: (projectId: string) => void | boolean | Promise<void | boolean>
    onOpenTaskRouting: () => void
  }> = {}
) {
  const onClose = vi.fn()
  render(
    <TaskFormModal
      isOpen
      onClose={onClose}
      onSubmit={onSubmit}
      agents={overrides.agents ?? [{ id: 'agent-1', name: 'Agent One', status: 'available' }]}
      projects={overrides.projects ?? [project]}
      selectedProjectId={
        Object.hasOwn(overrides, 'selectedProjectId') ? overrides.selectedProjectId! : project.id
      }
      selectedTaskGroupId={
        Object.hasOwn(overrides, 'selectedTaskGroupId') ? overrides.selectedTaskGroupId! : 'lane-1'
      }
      selectedTaskGroupName={
        Object.hasOwn(overrides, 'selectedTaskGroupName')
          ? overrides.selectedTaskGroupName!
          : 'Starter Queue'
      }
      onProjectChange={overrides.onProjectChange}
      onOpenTaskRouting={overrides.onOpenTaskRouting}
    />
  )
  return { onSubmit, onClose }
}

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('TaskFormModal', () => {
  test('uses beginner-friendly brief template prompts', () => {
    renderModal()

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
    renderModal(vi.fn(), { agents: [] })

    expect(
      screen.getByText(
        'No agents are online. You can create the task now; it will wait here until an agent comes online.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/dispatched?/i)).toBeNull()
  })

  test('guides busy-agent assignment without dispatch language', () => {
    renderModal(vi.fn(), {
      agents: [
        { id: 'agent-1', name: 'Busy Agent', status: 'busy' },
        { id: 'agent-2', name: 'Offline Agent', status: 'offline' },
      ],
    })

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
    renderModal()

    expect(screen.getByTestId('task-work-lane-readiness').textContent).toContain(
      'New tasks will wait in Starter Queue until an available agent picks them up.'
    )
    expect(
      screen.getByText(/Keep this choice when any available agent can do the work/i)
    ).toBeDefined()
    expect(screen.getByTestId('task-work-lane-readiness').textContent).not.toContain('is ready')
    expect(screen.getByTestId('task-work-lane-readiness').textContent).not.toContain(
      ['Agents', 'check', 'this', 'queue'].join(' ')
    )
    expect(screen.queryByText(/Leave this unassigned/i)).toBeNull()
  })

  test('explains task queue readiness before creating work', () => {
    const openTaskRouting = vi.fn()
    renderModal(vi.fn(), {
      selectedTaskGroupId: null,
      selectedTaskGroupName: null,
      onOpenTaskRouting: openTaskRouting,
    })

    expect(screen.getByTestId('task-work-lane-readiness')).toHaveTextContent(
      /Create a Task Queue First/i
    )
    expect(screen.getByText(/A task queue gives new work a place to wait/i)).toBeDefined()
    expect(screen.getByTestId('task-work-lane-readiness').textContent).not.toContain(
      ['agent', 'is', 'ready'].join(' ')
    )

    fireEvent.click(screen.getByRole('button', { name: /open task queues/i }))

    expect(openTaskRouting).toHaveBeenCalled()
  })

  test('labels unknown agent states without exposing backend status values', () => {
    renderModal(vi.fn(), {
      agents: [
        { id: 'agent-1', name: 'Ready Agent', status: 'available' },
        { id: 'agent-2', name: 'Starting Agent', status: 'starting_up' },
        { id: 'agent-3', name: 'Missing Status Agent', status: ' ' },
      ],
    })

    const readyOption = screen.getByRole('option', {
      name: 'Ready Agent (ready)',
    }) as HTMLOptionElement
    const unknownOption = screen.getByRole('option', {
      name: 'Starting Agent (not ready)',
    }) as HTMLOptionElement
    const missingStatusOption = screen.getByRole('option', {
      name: 'Missing Status Agent (refresh agent status)',
    }) as HTMLOptionElement

    expect(readyOption.disabled).toBe(false)
    expect(unknownOption.disabled).toBe(true)
    expect(missingStatusOption.disabled).toBe(true)
    expect(screen.queryByText(/starting_up/i)).toBeNull()
    expect(screen.queryByText(/status not reported/i)).toBeNull()
    expect(screen.queryByText(/Unknown/i)).toBeNull()
  })

  test('shows a beginner-safe title error before submitting', async () => {
    const { onSubmit } = renderModal()

    fireEvent.click(screen.getByRole('button', { name: /create task/i }))

    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(
        'Add a short title so the agent knows the goal.'
      )
    })
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('whitespace-only title shows the same beginner-safe error', async () => {
    const { onSubmit } = renderModal()

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: '   ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(
        'Add a short title so the agent knows the goal.'
      )
    })
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('valid title submits and closes without an error banner', async () => {
    const { onSubmit, onClose } = renderModal()

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Ship the fix' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit.mock.calls[0][0]).toMatchObject({
      title: 'Ship the fix',
      projectId: project.id,
    })
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole('alert')).toBeNull()
  })

  test('the submitted title is trimmed', async () => {
    const { onSubmit } = renderModal()

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: '  Ship the fix  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit.mock.calls[0][0].title).toBe('Ship the fix')
  })

  test('an onSubmit rejection shows a safe error and keeps the modal open', async () => {
    const { onSubmit, onClose } = renderModal(vi.fn().mockRejectedValue(new Error('boom')))

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Ship the fix' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('The task was not created')
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent('boom')
    expect(onClose).not.toHaveBeenCalled()
    expect(onSubmit).toHaveBeenCalledTimes(1)
  })

  test('a second failed submit with the same message scrolls the banner again', async () => {
    const scrollSpy = vi
      .spyOn(Element.prototype, 'scrollIntoView')
      .mockImplementation(() => undefined)
    renderModal()
    const submit = screen.getByRole('button', { name: /^create task$/i })

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: '   ' },
    })
    fireEvent.click(submit)
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/short title/i))
    const callsAfterFirst = scrollSpy.mock.calls.length
    expect(callsAfterFirst).toBeGreaterThan(0)

    fireEvent.click(submit)
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(callsAfterFirst))
    scrollSpy.mockRestore()
  })

  test('a task queue load failure reported by onProjectChange shows a retry message', async () => {
    const onProjectChange = vi.fn().mockResolvedValue(false)
    renderModal(vi.fn(), {
      projects: [project, otherProject],
      onProjectChange,
    })

    fireEvent.change(screen.getByLabelText(/^project$/i), { target: { value: otherProject.id } })

    await waitFor(() => expect(onProjectChange).toHaveBeenCalledWith(otherProject.id))
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(/task queues could not load/i)
    )
  })

  test('explains that a newly selected project is still preparing', async () => {
    const onProjectChange = vi.fn(() => new Promise<void>(() => undefined))
    renderModal(vi.fn(), {
      projects: [project, otherProject],
      onProjectChange,
    })

    fireEvent.change(screen.getByLabelText(/^project$/i), { target: { value: otherProject.id } })

    await waitFor(() => expect(onProjectChange).toHaveBeenCalledWith(otherProject.id))

    const readiness = screen.getByTestId('task-work-lane-readiness')
    expect(readiness).toHaveTextContent('Preparing This Project')
    expect(readiness).toHaveTextContent(
      'Forge is loading the task queue for this project. Wait a moment before creating the task.'
    )
    expect(readiness).not.toHaveTextContent('Create a Task Queue First')
    expect(screen.getByRole('button', { name: /preparing project/i })).toBeDisabled()
  })
})
