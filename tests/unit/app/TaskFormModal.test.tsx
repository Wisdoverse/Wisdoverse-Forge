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
    agents: { id: string; name: string; status: string; capabilities?: string[] }[]
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

function openTaskOptions() {
  fireEvent.click(screen.getByText('Task options'))
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })
  return { promise, resolve }
}

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('TaskFormModal', () => {
  test('shows starter templates by default so new users can start from examples', () => {
    renderModal()

    expect(screen.getByRole('button', { name: /hide task writing help/i })).toHaveAttribute(
      'aria-expanded',
      'true'
    )
    expect(
      screen.getByText('Use a starter template when you are not sure what to write.')
    ).toBeDefined()
    expect(screen.getByText('Start with a task template')).toBeDefined()
    expect(screen.getByText('Fills in a safe first draft')).toBeDefined()
    expect(screen.getByText(/project, a task queue, and enough detail/i)).toBeDefined()
    expect(screen.getByRole('group', { name: /task templates/i })).toBeDefined()
    expect(screen.getByText('A clear task has three plain-language parts')).toBeDefined()
    expect(screen.getByText('Goal')).toBeDefined()
    expect(screen.getByText('Place')).toBeDefined()
    expect(screen.getByText('Proof')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /hide task writing help/i }))

    expect(screen.getByRole('button', { name: /need help writing the task/i })).toHaveAttribute(
      'aria-expanded',
      'false'
    )
    expect(screen.queryByText('Start with a task template')).toBeNull()
    expect(screen.queryByText('Fills in a safe first draft')).toBeNull()
    expect(screen.queryByRole('group', { name: /task templates/i })).toBeNull()
    expect(screen.queryByText('A clear task has three plain-language parts')).toBeNull()
    expect(screen.queryByText('Goal')).toBeNull()
    expect(screen.queryByText('Place')).toBeNull()
    expect(screen.queryByText('Proof')).toBeNull()
    expect(screen.queryByText(/scope and proof/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /need help writing the task/i }))

    expect(screen.getByRole('button', { name: /hide task writing help/i })).toHaveAttribute(
      'aria-expanded',
      'true'
    )
    expect(screen.getByText('Start with a task template')).toBeDefined()
    expect(screen.getByText('Fills in a safe first draft')).toBeDefined()
    expect(screen.getByText(/project, a task queue, and enough detail/i)).toBeDefined()
    expect(screen.getByRole('group', { name: /task templates/i })).toBeDefined()
    expect(screen.getByText('A clear task has three plain-language parts')).toBeDefined()
    expect(screen.getByText('Goal')).toBeDefined()
    expect(screen.getByText('Place')).toBeDefined()
    expect(screen.getByText('Proof')).toBeDefined()

    expect(screen.getByText('Add something')).toBeDefined()
    expect(screen.getByText('Fix a problem')).toBeDefined()
    expect(screen.getByText('Find the cause')).toBeDefined()
    expect(screen.getByText('Check a change')).toBeDefined()
    expect(screen.queryByText(/^Feature$/)).toBeNull()
    expect(screen.queryByText(/^Bug$/)).toBeNull()
    expect(screen.queryByText(/^Investigate$/)).toBeNull()
    expect(screen.queryByText(/^Review$/)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /add something/i }))

    expect(screen.getByLabelText(/what should the agent finish/i)).toHaveValue(
      'Add one focused change'
    )
    const description = screen.getByLabelText(
      /details the agent should know/i
    ) as HTMLTextAreaElement
    expect(description.value).toContain('What should change:')
    expect(description.value).toContain('Where to work:')
    expect(description.value).toContain('What to avoid:')
    expect(description.value).toContain('Done when:')
    expect(description.value).toContain('Describe what you want to see or use after this is done.')
    expect(description.value).toContain(
      'Say what should be visible, ready to use, or easy to check.'
    )
    expect(description.value).not.toContain('Describe the screen, command, or behavior to add.')
    expect(description.value).not.toContain(
      'Say what should be visible, passing, or ready to review.'
    )
    expect(description.value).not.toMatch(/^-\s*$/m)
    expect(description.value).not.toContain('Scope:')
    expect(description.value).not.toContain('Constraints:')
    expect(description.value).not.toContain('Evidence:')

    fireEvent.click(screen.getByRole('button', { name: /fix a problem/i }))
    expect(
      (screen.getByLabelText(/details the agent should know/i) as HTMLTextAreaElement).value
    ).toContain('Say how you will know the problem is fixed.')
    expect(
      (screen.getByLabelText(/details the agent should know/i) as HTMLTextAreaElement).value
    ).not.toMatch(/^-\s*$/m)

    fireEvent.click(screen.getByRole('button', { name: /find the cause/i }))
    expect(
      (screen.getByLabelText(/details the agent should know/i) as HTMLTextAreaElement).value
    ).toContain('Add what you already tried or noticed.')
    expect(
      (screen.getByLabelText(/details the agent should know/i) as HTMLTextAreaElement).value
    ).not.toMatch(/^-\s*$/m)

    fireEvent.click(screen.getByRole('button', { name: /check a change/i }))
    const reviewDescription = screen.getByLabelText(
      /details the agent should know/i
    ) as HTMLTextAreaElement
    expect(reviewDescription.value).toContain('Name what changed and where a user would see it.')
    expect(reviewDescription.value).toContain(
      'Say what you want the agent to check, such as a screen, result, or sign-in step.'
    )
    expect(reviewDescription.value).toContain(
      'Ask for what is safe, what needs fixing, and what to do next.'
    )
    expect(reviewDescription.value).toContain('Change to check:')
    expect(reviewDescription.value).toContain('What to check:')
    expect(reviewDescription.value).toContain('Answer needed:')
    expect(reviewDescription.value).not.toContain('Change to review:')
    expect(reviewDescription.value).not.toContain('Name the PR, branch')
    expect(reviewDescription.value).not.toContain(
      'Name the change, request, files, screen, or behavior.'
    )
    expect(reviewDescription.value).not.toContain('Add tests, commands, or manual checks.')
    expect(reviewDescription.value).not.toContain(
      'Ask for a short verdict, issues, and final recommendation.'
    )
    expect(reviewDescription.value).not.toContain('release readiness')
    expect(reviewDescription.value).not.toMatch(/^-\s*$/m)
  })

  test('resets task writing help when the modal reopens', () => {
    const onClose = vi.fn()
    const onSubmit = vi.fn()
    const props = {
      onClose,
      onSubmit,
      agents: [{ id: 'agent-1', name: 'Agent One', status: 'available' }],
      projects: [project],
      selectedProjectId: project.id,
      selectedTaskGroupId: 'lane-1',
      selectedTaskGroupName: 'Starter Queue',
    }
    const { rerender } = render(<TaskFormModal isOpen {...props} />)

    expect(screen.getByRole('button', { name: /hide task writing help/i })).toHaveAttribute(
      'aria-expanded',
      'true'
    )
    fireEvent.click(screen.getByRole('button', { name: /hide task writing help/i }))
    expect(screen.getByRole('button', { name: /need help writing the task/i })).toHaveAttribute(
      'aria-expanded',
      'false'
    )

    rerender(<TaskFormModal isOpen={false} {...props} />)
    rerender(<TaskFormModal isOpen {...props} />)

    expect(screen.getByRole('button', { name: /hide task writing help/i })).toHaveAttribute(
      'aria-expanded',
      'true'
    )
    expect(screen.getByText('Start with a task template')).toBeDefined()
  })

  test('does not treat template helper prompts as finished task details', () => {
    renderModal()

    fireEvent.click(screen.getByRole('button', { name: /add something/i }))

    expect(screen.getByTestId('task-brief-cue-goal')).toHaveTextContent('Add')
    expect(screen.getByTestId('task-brief-cue-goal')).toHaveTextContent(
      'Replace the template title with the specific result you want.'
    )
    expect(screen.getByTestId('task-brief-cue-where')).toHaveTextContent('Add')
    expect(screen.getByTestId('task-brief-cue-where')).toHaveTextContent(
      'Name the page, screen, file, or area to check first.'
    )
    expect(screen.getByTestId('task-brief-cue-done')).toHaveTextContent('Add')
    expect(screen.getByTestId('task-brief-cue-done')).toHaveTextContent(
      'Add the simple check, screenshot, or result that proves it is done.'
    )

    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value: 'Where to work:\n- src/app/features/board\n\nDone when:\n- Task form test passes',
      },
    })

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Add task template readiness checks' },
    })

    expect(screen.getByTestId('task-brief-cue-goal')).toHaveTextContent('Ready')
    expect(screen.getByTestId('task-brief-cue-where')).toHaveTextContent('Ready')
    expect(screen.getByTestId('task-brief-cue-done')).toHaveTextContent('Ready')
  })

  test('shows what a new agent still needs before the task is clear', () => {
    renderModal()

    expect(screen.getByTestId('task-brief-checklist')).toHaveTextContent(
      'Make this task easy to start'
    )
    expect(screen.getByTestId('task-submit-preview')).toHaveTextContent('What happens after this')
    expect(screen.getByTestId('task-submit-preview')).toHaveTextContent(
      'After you create it, the next ready agent can start it from this project.'
    )
    expect(screen.getByTestId('task-submit-preview')).not.toHaveTextContent('pick it up')
    expect(screen.getByTestId('task-brief-cue-goal')).toHaveTextContent('Add')
    expect(screen.getByTestId('task-brief-cue-goal')).toHaveTextContent(
      'Write one sentence for the result you want.'
    )
    expect(screen.getByTestId('task-brief-cue-where')).toHaveTextContent(
      'Name the page, screen, file, or area to check first.'
    )
    expect(screen.getByTestId('task-brief-cue-done')).toHaveTextContent(
      'Add the simple check, screenshot, or result that proves it is done.'
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

  test('keeps priority and manual assignment in task options until requested', () => {
    renderModal()

    expect(screen.queryByLabelText(/^priority$/i)).toBeNull()
    expect(screen.queryByLabelText(/who should start it/i)).toBeNull()
    expect(screen.getByText(/normal priority/i)).toBeDefined()
    expect(screen.getByText(/next ready agent starts it/i)).toBeDefined()
    expect(screen.queryByText(/automatic agent selection/i)).toBeNull()

    openTaskOptions()

    expect(screen.getByLabelText(/^priority$/i)).toBeDefined()
    expect(screen.getByLabelText(/who should start it/i)).toBeDefined()
    expect(screen.getByRole('option', { name: /let the next ready agent start it/i })).toBeDefined()
  })

  test('routes no-agent setup without dispatch language', () => {
    const onOpenAgentSetup = vi.fn()
    renderModal(vi.fn(), { agents: [], onOpenAgentSetup })

    expect(screen.getByText('Connect an agent before this task can start')).toBeDefined()
    expect(screen.getByText(/save the task now/i)).toBeDefined()
    expect(
      screen.getByText(
        'Save the task now. It will wait here until an agent is ready. To start it sooner, open Agents.'
      )
    ).toBeDefined()
    expect(screen.getByText(/to start it sooner, open Agents/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /open agents/i })).toBeDefined()
    openTaskOptions()
    expect(screen.getByText(/This task will wait here until an agent is ready/i)).toBeDefined()
    expect(screen.getByTestId('task-submit-preview')).toHaveTextContent(
      'After you save, the task waits here until an agent is ready.'
    )
    expect(screen.queryByText('No agents are online')).toBeNull()
    expect(screen.queryByText(/open agent setup/i)).toBeNull()
    expect(screen.queryByText(/Create the task now/i)).toBeNull()
    expect(screen.queryByText(/dispatched?/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /open agents/i }))

    expect(onOpenAgentSetup).toHaveBeenCalledTimes(1)
  })

  test('does not tell users to save a waiting task before project setup is ready', () => {
    const onOpenAgentSetup = vi.fn()
    renderModal(vi.fn(), {
      agents: [],
      projects: [],
      selectedProjectId: null,
      selectedTaskGroupId: null,
      selectedTaskGroupName: null,
      onOpenAgentSetup,
    })

    expect(screen.getByText('Connect an agent before this task can start')).toBeDefined()
    expect(
      screen.getByText(
        'Create a project and set up a task queue first. Then this task can wait here until an agent is ready. To fix agent setup now, open Agents.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/Save the task now/i)).toBeNull()
    expect(screen.getByRole('button', { name: /open agents/i })).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /open agents/i }))

    expect(onOpenAgentSetup).toHaveBeenCalledTimes(1)
  })

  test('guides project setup before the first task', async () => {
    const onOpenProjectSettings = vi.fn()
    const { onSubmit } = renderModal(vi.fn(), {
      projects: [],
      selectedProjectId: null,
      selectedTaskGroupId: null,
      selectedTaskGroupName: null,
      onOpenProjectSettings,
    })

    expect(screen.getByText('Create a project before sending tasks')).toBeDefined()
    expect(screen.getByText(/projects keep each task/i)).toBeDefined()
    expect(screen.queryByText(/No projects available/i)).toBeNull()

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Ship the fix' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('Open project settings before creating a task.')
    fireEvent.click(screen.getAllByRole('button', { name: /open project settings/i }).at(-1)!)

    expect(onOpenProjectSettings).toHaveBeenCalledTimes(1)
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('shows project setup card action before the first task', () => {
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

  test('routes busy-agent setup without dispatch language', () => {
    const onOpenAgentSetup = vi.fn()
    renderModal(vi.fn(), {
      agents: [
        { id: 'agent-1', name: 'Busy Agent', status: 'busy' },
        { id: 'agent-2', name: 'Offline Agent', status: 'offline' },
      ],
      onOpenAgentSetup,
    })

    expect(screen.getByText('Start or connect an agent before this task can start')).toBeDefined()
    expect(screen.getByText(/wait here until one of your agents is ready/i)).toBeDefined()
    expect(screen.getByText(/to start it sooner, open Agents/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /save task to wait/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /open agents/i })).toBeDefined()
    openTaskOptions()
    expect(screen.getByText(/This task will wait here until an agent is ready/i)).toBeDefined()
    expect(screen.queryByText('No agents are available right now')).toBeNull()
    expect(screen.getByRole('option', { name: /let the next ready agent start it/i })).toBeDefined()
    expect(screen.queryByText(/any available agent can do the work/i)).toBeNull()
    expect(screen.queryByText(/unassigned/i)).toBeNull()
    expect(screen.getByText(/people are waiting on it now/i)).toBeDefined()
    expect(screen.queryByText(/dispatch/i)).toBeNull()
    expect(screen.queryByText(/Keep the default choice so the next available agent/i)).toBeNull()
    expect(screen.queryByText(/Create the task now/i)).toBeNull()
    expect(screen.queryByText(/open agent setup/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /open agents/i }))

    expect(onOpenAgentSetup).toHaveBeenCalledTimes(1)
  })

  test('does not count chat-only agents as ready for Tasks', () => {
    const onOpenAgentSetup = vi.fn()
    renderModal(vi.fn(), {
      agents: [{ id: 'agent-1', name: 'Chat Helper', status: 'available', capabilities: [] }],
      onOpenAgentSetup,
    })

    expect(screen.getByText('Create a task-ready agent before this task can start')).toBeDefined()
    expect(
      screen.getByText(
        'Simple chat agents answer questions in Chat. For Tasks, open Agents and create or start a Project files or This computer agent.'
      )
    ).toBeDefined()
    openTaskOptions()
    expect(screen.getByText(/0 ready/i)).toBeDefined()
    expect(
      screen.getByRole('option', {
        name: 'Chat Helper (chat only - cannot take Tasks)',
      })
    ).toBeDisabled()
    expect(screen.getByRole('button', { name: /open agents/i })).toBeDefined()
    expect(screen.queryByText('1 ready')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /open agents/i }))

    expect(onOpenAgentSetup).toHaveBeenCalledTimes(1)
  })

  test('explains a ready task queue without internal checking language', () => {
    renderModal()

    expect(screen.getByTestId('task-work-lane-readiness').textContent).toContain(
      'New tasks will wait in Starter task queue until a ready agent starts them.'
    )
    openTaskOptions()
    expect(screen.getByText(/1 ready/i)).toBeDefined()
    expect(
      screen.getByText(/Use the next ready agent when any ready agent can do the work/i)
    ).toBeDefined()
    expect(screen.queryByText(/automatic selection/i)).toBeNull()
    expect(screen.queryByText(/Keep this choice when any available agent/i)).toBeNull()
    expect(screen.getByTestId('task-work-lane-readiness').textContent).not.toContain('is ready')
    expect(screen.getByTestId('task-work-lane-readiness').textContent).not.toContain(
      ['Agents', 'check', 'this', 'queue'].join(' ')
    )
    expect(screen.getByTestId('task-work-lane-readiness')).not.toHaveTextContent('Starter Queue')
    expect(screen.queryByText(/Leave this unassigned/i)).toBeNull()
  })

  test('explains the task queue needed before creating work', async () => {
    const openTaskRouting = vi.fn()
    const { onSubmit } = renderModal(vi.fn(), {
      selectedTaskGroupId: null,
      selectedTaskGroupName: null,
      onOpenTaskRouting: openTaskRouting,
    })

    expect(screen.getByTestId('task-work-lane-readiness')).toHaveTextContent(
      /Set up a task queue before creating this task/i
    )
    expect(screen.getByText(/Create one place for new tasks to wait/i)).toBeDefined()
    expect(screen.queryByText(/Create one place for new work to wait/i)).toBeNull()
    const readiness = screen.getByTestId('task-work-lane-readiness')
    expect(readiness).toHaveTextContent('Open Agents.')
    expect(readiness).toHaveTextContent('Choose this project: Starter Project.')
    expect(readiness).toHaveTextContent('Create one task queue for new tasks.')
    expect(readiness).toHaveTextContent(
      'Come back here. Success looks like this card saying Task can be created.'
    )
    expect(screen.getByTestId('task-work-lane-readiness').textContent).not.toContain(
      ['agent', 'is', 'ready'].join(' ')
    )
    expect(screen.getByTestId('task-work-lane-readiness')).not.toHaveTextContent(
      /Open task queues before creating this task/i
    )

    fireEvent.click(screen.getByRole('button', { name: /set up task queue/i }))

    expect(openTaskRouting).toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Ship the fix' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('Set up a task queue before saving this task.')
    fireEvent.click(screen.getAllByRole('button', { name: /set up task queue/i }).at(-1)!)

    expect(openTaskRouting).toHaveBeenCalledTimes(2)
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('labels unknown agent states without exposing backend status values', () => {
    renderModal(vi.fn(), {
      agents: [
        { id: 'agent-1', name: 'Ready Agent', status: 'available' },
        { id: 'agent-2', name: 'Starting Agent', status: 'starting_up' },
        { id: 'agent-3', name: 'Missing Status Agent', status: ' ' },
      ],
    })

    openTaskOptions()
    const readyOption = screen.getByRole('option', {
      name: 'Ready Agent (ready)',
    }) as HTMLOptionElement
    const unknownOption = screen.getByRole('option', {
      name: 'Starting Agent (not ready)',
    }) as HTMLOptionElement
    const missingStatusOption = screen.getByRole('option', {
      name: 'Missing Status Agent (check agent status)',
    }) as HTMLOptionElement

    expect(readyOption.disabled).toBe(false)
    expect(unknownOption.disabled).toBe(true)
    expect(missingStatusOption.disabled).toBe(true)
    expect(screen.queryByText(/starting_up/i)).toBeNull()
    expect(screen.queryByText(/status not reported/i)).toBeNull()
    expect(screen.queryByText(/Unknown/i)).toBeNull()
  })

  test('blocks a stale non-ready agent choice with a plain next step', async () => {
    const { onSubmit } = renderModal(vi.fn(), {
      agents: [
        { id: 'agent-1', name: 'Ready Agent', status: 'available' },
        { id: 'agent-2', name: 'Busy Agent', status: 'busy' },
      ],
    })

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Ship the fix' },
    })
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value: 'Where to work:\n- src/app/features/board\n\nDone when:\n- Task form test passes',
      },
    })
    openTaskOptions()
    fireEvent.change(screen.getByLabelText(/who should start it/i), {
      target: { value: 'agent-2' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Choose a ready agent, or leave this set to Let the next ready agent start it.'
    )
    await waitFor(() => expect(screen.getByLabelText(/who should start it/i)).toHaveFocus())
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('shows a beginner-safe title error before submitting', async () => {
    const { onSubmit } = renderModal()

    fireEvent.click(screen.getByRole('button', { name: /create task/i }))

    await waitFor(() => {
      const alert = screen.getByRole('alert')
      expect(alert).toHaveAttribute('aria-live', 'polite')
      expect(alert).toHaveTextContent('Add a short title so the agent knows the goal.')
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
      const alert = screen.getByRole('alert')
      expect(alert).toHaveAttribute('aria-live', 'polite')
      expect(alert).toHaveTextContent('Add a short title so the agent knows the goal.')
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

  test('names task creation progress after the user submits', async () => {
    const request = deferred<void>()
    const { onSubmit } = renderModal(vi.fn().mockReturnValueOnce(request.promise))

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Ship the fix' },
    })
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value: 'Where to work:\n- src/app/features/board\n\nDone when:\n- Task form test passes',
      },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    expect(screen.getByRole('button', { name: /creating task/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^Creating\.\.\.$/i })).toBeNull()

    request.resolve()
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
  })

  test('names waiting-task save progress when no agent is ready yet', async () => {
    const request = deferred<void>()
    const { onSubmit } = renderModal(vi.fn().mockReturnValueOnce(request.promise), { agents: [] })

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Save work for later' },
    })
    fireEvent.change(screen.getByLabelText(/details the agent should know/i), {
      target: {
        value: 'Where to work:\n- src/app/features/board\n\nDone when:\n- Task waits for an agent',
      },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save task to wait$/i }))

    expect(screen.getByRole('button', { name: /saving task to wait/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^Saving\.\.\.$/i })).toBeNull()

    request.resolve()
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
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
    expect(confirmation).toHaveTextContent('Add missing details before this task starts.')
    expect(confirmation).toHaveTextContent('Missing: where to work and done when')
    expect(confirmation).toHaveTextContent('Best next step: add where to work and done when.')
    expect(confirmation).toHaveTextContent(
      'If you choose Create task anyway, the agent may pause to ask follow-up questions.'
    )
    expect(confirmation).not.toHaveTextContent('may need to ask what to check or where to work')
    expect(confirmation).not.toHaveTextContent('This task may be hard for an agent to finish.')
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

  test('uses save wording when a waiting task has missing brief details', async () => {
    const { onSubmit, onClose } = renderModal(vi.fn(), { agents: [] })

    fireEvent.change(screen.getByLabelText(/what should the agent finish/i), {
      target: { value: 'Ship the fix' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save task to wait$/i }))

    expect(onSubmit).not.toHaveBeenCalled()
    expect(onClose).not.toHaveBeenCalled()
    const confirmation = await screen.findByTestId('task-brief-confirmation')
    expect(confirmation).toHaveTextContent('Add missing details before this task starts.')
    expect(confirmation).toHaveTextContent('Best next step: add where to work and done when.')
    expect(confirmation).toHaveTextContent(
      'If you choose Save task anyway, the agent may pause to ask follow-up questions.'
    )
    expect(confirmation).not.toHaveTextContent('may need to ask what to check or where to work')
    expect(confirmation).not.toHaveTextContent('This task may be hard for an agent to finish.')
    expect(screen.getByRole('button', { name: /^save task anyway$/i })).toBeDefined()
    expect(screen.queryByRole('button', { name: /^create task anyway$/i })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /^save task anyway$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
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
    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).not.toHaveTextContent('boom')
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
    await waitFor(() => {
      const alert = screen.getByRole('alert')
      expect(alert).toHaveAttribute('aria-live', 'polite')
      expect(alert).toHaveTextContent(/short title/i)
    })
    await waitFor(() => expect(scrollSpy).toHaveBeenCalled())
    const callsAfterFirst = scrollSpy.mock.calls.length

    fireEvent.click(submit)
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(callsAfterFirst))
    scrollSpy.mockRestore()
  })

  test('a waiting-place load failure reported by onProjectChange starts with the next step', async () => {
    const onProjectChange = vi.fn().mockResolvedValue(false)
    renderModal(vi.fn(), {
      projects: [project, otherProject],
      onProjectChange,
    })

    fireEvent.change(screen.getByLabelText(/^project$/i), { target: { value: otherProject.id } })

    await waitFor(() => expect(onProjectChange).toHaveBeenCalledWith(otherProject.id))
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(
        'Select the project again to find the task queue. If it still does not load, open the Tasks page again or ask an owner to check the task queue in this project.'
      )
    )
    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).not.toHaveTextContent('task routing setup')
    expect(alert).not.toHaveTextContent(/task queues could not load/i)
    expect(alert).not.toHaveTextContent(/load task queues/i)
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
    expect(readiness).toHaveTextContent('Checking where new tasks will wait')
    expect(readiness).toHaveTextContent(
      'Wait a moment while Forge finds the task queue for this project.'
    )
    expect(readiness).not.toHaveTextContent('Create a Task Queue First')
    expect(readiness).not.toHaveTextContent('Loading this project')
    expect(readiness).not.toHaveTextContent('Preparing This Project')
    expect(screen.getByRole('button', { name: /preparing project/i })).toBeDisabled()
  })
})
