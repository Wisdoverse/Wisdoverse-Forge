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
    onOpenAgentSetup: () => void
    onOpenProjectSettings: () => void
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
      onOpenAgentSetup={overrides.onOpenAgentSetup}
      onOpenProjectSettings={overrides.onOpenProjectSettings}
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
    expect(screen.getAllByText('Where to work').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Done when').length).toBeGreaterThan(0)
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

  test('shows what a new agent still needs before the task is clear', () => {
    renderModal()

    expect(screen.getByTestId('task-brief-checklist')).toHaveTextContent(
      'Make this task easy to pick up'
    )
    expect(screen.getByTestId('task-brief-cue-goal')).toHaveTextContent('Add')
    expect(screen.getByTestId('task-brief-cue-goal')).toHaveTextContent(
      'Write one sentence for the result you want.'
    )
    expect(screen.getByTestId('task-brief-cue-where')).toHaveTextContent(
      'Name the files, screen, folder, or area to check first.'
    )
    expect(screen.getByTestId('task-brief-cue-done')).toHaveTextContent(
      'Add the test, screenshot, output, or result that proves it is done.'
    )

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Fix the login error' },
    })
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value: 'Where to work:\n- src/app/features/auth\n\nDone when:\n- Login test passes',
      },
    })

    expect(screen.getByTestId('task-brief-cue-goal')).toHaveTextContent('Ready')
    expect(screen.getByTestId('task-brief-cue-where')).toHaveTextContent('Ready')
    expect(screen.getByTestId('task-brief-cue-done')).toHaveTextContent('Ready')
    expect(screen.getByTestId('task-brief-cue-done')).toHaveTextContent(
      'The agent knows how success will be checked.'
    )
  })

  test('routes no-agent setup without dispatch language', () => {
    const onOpenAgentSetup = vi.fn()
    renderModal(vi.fn(), { agents: [], onOpenAgentSetup })

    expect(screen.getByText('No agents are online')).toBeDefined()
    expect(screen.getByText(/create the task now/i)).toBeDefined()
    expect(screen.queryByText(/dispatched?/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /open agent setup/i }))

    expect(onOpenAgentSetup).toHaveBeenCalledTimes(1)
  })

  test('guides project setup before the first task', () => {
    const onOpenProjectSettings = vi.fn()
    renderModal(vi.fn(), {
      projects: [],
      selectedProjectId: null,
      selectedTaskGroupId: null,
      selectedTaskGroupName: null,
      onOpenProjectSettings,
    })

    expect(screen.getByText('Create a project before sending tasks')).toBeDefined()
    expect(screen.getByText(/projects keep each task/i)).toBeDefined()
    expect(screen.queryByText(/No projects available/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /open project settings/i }))

    expect(onOpenProjectSettings).toHaveBeenCalledTimes(1)
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
      /Create a task queue before sending work/i
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
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value: 'Where to work:\n- src/app/features/board\n\nDone when:\n- Task form test passes',
      },
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

  test('asks for one confirmation before creating a task with missing brief details', async () => {
    const { onSubmit, onClose } = renderModal()

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Ship the fix' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    expect(onSubmit).not.toHaveBeenCalled()
    expect(onClose).not.toHaveBeenCalled()
    const confirmation = await screen.findByTestId('task-brief-confirmation')
    expect(confirmation).toHaveTextContent('This task may be hard for an agent to finish.')
    expect(confirmation).toHaveTextContent('Add where to work and done when')
    expect(screen.getByRole('button', { name: /^create task anyway$/i })).toBeDefined()
    expect(screen.queryByRole('button', { name: /^create anyway$/i })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /^create task anyway$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit.mock.calls[0][0]).toMatchObject({
      title: 'Ship the fix',
      projectId: project.id,
    })
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1))
  })

  test('clears the incomplete brief confirmation when the user adds missing details', async () => {
    const { onSubmit } = renderModal()

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Ship the fix' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))
    expect(await screen.findByTestId('task-brief-confirmation')).toBeDefined()

    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value: 'Where to work:\n- src/app/features/board\n\nDone when:\n- Task form test passes',
      },
    })

    expect(screen.queryByTestId('task-brief-confirmation')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
  })

  test('the submitted title is trimmed', async () => {
    const { onSubmit } = renderModal()

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: '  Ship the fix  ' },
    })
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value: 'Where to work:\n- src/app/features/board\n\nDone when:\n- Task form test passes',
      },
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
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value: 'Where to work:\n- src/app/features/board\n\nDone when:\n- Task form test passes',
      },
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

  test('a task queue load failure reported by onProjectChange starts with the next step', async () => {
    const onProjectChange = vi.fn().mockResolvedValue(false)
    renderModal(vi.fn(), {
      projects: [project, otherProject],
      onProjectChange,
    })

    fireEvent.change(screen.getByLabelText(/^project$/i), { target: { value: otherProject.id } })

    await waitFor(() => expect(onProjectChange).toHaveBeenCalledWith(otherProject.id))
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(
        'Select the project again to load task queues. If it still does not load, refresh the board or ask an owner to check task queue setup.'
      )
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/task queues could not load/i)
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
