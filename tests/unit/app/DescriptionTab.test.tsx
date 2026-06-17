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
      'Choose an agent before this task can start.'
    )
    expect(screen.getByTestId('task-next-action').textContent).toContain(
      'Choose an available agent, review the suggested saved notes and instructions, then send the task.'
    )
    expect(screen.getByTestId('task-next-action').textContent).not.toContain('when ready')
    expect(screen.getByTestId('task-next-action').textContent).not.toMatch(/suggested\s+context/)
    expect(screen.getByTestId('task-work-review').textContent).not.toContain('leave the backlog')
    expect(screen.getByTestId('task-work-review').textContent).not.toContain('publish with context')
  })

  test('does not call a task unassigned when only the agent id is loaded', () => {
    render(<DescriptionTab task={{ ...mockTask, assignedTo: 'agent-1' }} />)

    expect(screen.getByText('Agent details loading')).toBeDefined()
    expect(screen.getByText('Ready to send')).toBeDefined()
    expect(screen.getByTestId('task-assignment-guidance').textContent).toBe(
      'An agent was chosen, but its name has not loaded yet. Refresh this task so you can confirm the right agent before sending it.'
    )
    expect(screen.getByTestId('task-next-action').textContent).toContain(
      'Review the brief, then send it to this agent.'
    )
    expect(screen.getByTestId('task-next-action').textContent).not.toContain('when ready')
    expect(screen.queryByText('Assigned agent')).toBeNull()
    expect(screen.queryByText('Unassigned')).toBeNull()
    expect(screen.queryByText(/dispatch/i)).toBeNull()
  })

  test('guides title-only saved tasks before sending them to an agent', () => {
    render(
      <DescriptionTab
        task={{
          ...mockTask,
          assignedTo: 'agent-1',
          params: { ...mockTask.params, message: '' },
        }}
      />
    )

    expect(screen.getByText('Add details before sending')).toBeDefined()
    expect(screen.getByTestId('task-next-action').textContent).toContain(
      'This task only has a title. Add what to finish, where to look, and how you will check it before sending.'
    )
    expect(
      screen.getByText(
        'Only the task title was saved. Before sending, add what to finish, where to look, and how you will check it.'
      )
    ).toBeDefined()
    expect(screen.getByTestId('task-next-action').textContent).not.toContain('Review the brief')
    expect(screen.queryByText(/Open Updates to see what was asked/i)).toBeNull()
  })

  test('guides title-only saved tasks before choosing an agent', () => {
    render(
      <DescriptionTab
        task={{
          ...mockTask,
          params: { ...mockTask.params, message: '   ' },
        }}
      />
    )

    expect(screen.getByText('Add details and choose an agent')).toBeDefined()
    expect(screen.getByTestId('task-next-action').textContent).toContain(
      'This task only has a title. Add what to finish, where to look, and how to check it, then choose an agent.'
    )
    expect(screen.getByTestId('task-next-action').textContent).not.toContain(
      'review the suggested saved notes'
    )
  })

  test('explains waiting tasks without internal runtime language', () => {
    render(<DescriptionTab task={{ ...mockTask, state: 'queued', assignedTo: 'agent-1' }} />)

    expect(screen.getByText('Waiting to start')).toBeDefined()
    expect(screen.getByText('Waiting for the agent to start')).toBeDefined()
    expect(screen.getByTestId('task-next-action').textContent).toContain(
      'If this stays here, open Updates to check the last activity, then choose another agent if needed.'
    )
    expect(screen.queryByText('Queued')).toBeNull()
    expect(screen.queryByText(/execution|runtime/i)).toBeNull()
  })

  test('explains how to start a waiting task that has no agent', () => {
    render(<DescriptionTab task={{ ...mockTask, state: 'queued' }} />)

    expect(screen.getByText('Waiting for an available agent')).toBeDefined()
    expect(screen.getByTestId('task-next-action').textContent).toContain(
      'If this stays here, choose or start an agent so the task has someone to begin the work.'
    )
    expect(screen.getByTestId('task-next-action').textContent).not.toMatch(/queue|pick it up/i)
  })

  test('keeps result and context actions available for completed tasks', () => {
    const onOpenResult = vi.fn()
    const onOpenContext = vi.fn()
    const onDraftSkill = vi.fn()

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
        onDraftSkill={onDraftSkill}
      />
    )

    expect(screen.getByTestId('task-assignment-guidance').textContent).toBe(
      'This agent will handle the next step for this task.'
    )

    fireEvent.click(screen.getByRole('button', { name: /open result files/i }))
    fireEvent.click(screen.getByRole('button', { name: /^review what was used/i }))
    fireEvent.click(screen.getByRole('button', { name: /review save ideas/i }))
    fireEvent.click(screen.getByRole('button', { name: /draft saved instruction/i }))

    expect(onOpenResult).toHaveBeenCalledOnce()
    expect(onOpenContext).toHaveBeenCalledTimes(2)
    expect(onDraftSkill).toHaveBeenCalledOnce()
    expect(screen.getByText('Reuse what worked')).toBeDefined()
    expect(screen.getByTestId('task-next-action').textContent).toContain(
      'check what the agent reused'
    )
    expect(
      screen.getByText('After review, save the repeatable steps if future tasks should reuse them.')
    ).toBeDefined()
    expect(
      screen.getByText('Save the repeatable steps only when they should help future tasks.')
    ).toBeDefined()
    expect(screen.getByText('Check work')).toBeDefined()
    expect(
      screen.getByText('Open result files or what the agent used before accepting.')
    ).toBeDefined()
    expect(screen.getByText('Result files')).toBeDefined()
    expect(screen.getByText('1 saved note or instruction helped this task.')).toBeDefined()
    expect(screen.queryByText(new RegExp(['saved context', 'item'].join('\\s+'), 'i'))).toBeNull()
    expect(screen.queryByText('Evidence')).toBeNull()
    expect(screen.queryByText(/result files and evidence/i)).toBeNull()
    const previousReuseCopy = new RegExp(['Completed work', 'saved instructions'].join('.*'), 'i')
    expect(screen.queryByText(previousReuseCopy)).toBeNull()
    expect(screen.queryByText('Reusable learning')).toBeNull()
    expect(screen.queryByText(/saved guidance/i)).toBeNull()
    expect(screen.queryByText(/governed skill/i)).toBeNull()
    expect(screen.queryByText(new RegExp(['Draft a', 'skill'].join('\\s+')))).toBeNull()
    expect(screen.queryByText(new RegExp(['result', 'artifact'].join('\\s+'), 'i'))).toBeNull()
  })

  test('uses plain context wording before a task has saved context', () => {
    render(<DescriptionTab task={mockTask} />)

    expect(
      screen.getByText(
        'Saved notes, work history, and save-for-next-time ideas appear here while the task is active.'
      )
    ).toBeDefined()
    expect(
      screen.queryByText(
        'Saved notes, run details, and save-for-next-time ideas appear here as the task runs.'
      )
    ).toBeNull()
    expect(screen.queryByText(/after the run finishes/i)).toBeNull()
    expect(screen.queryByText(/next run for this task/i)).toBeNull()
    expect(screen.queryByText(new RegExp(['Saved', 'memories'].join('\\s+'), 'i'))).toBeNull()
    expect(
      screen.queryByText(new RegExp(['saved instruction', 'suggestions'].join('\\s+'), 'i'))
    ).toBeNull()
    expect(
      screen.queryByText(
        new RegExp(
          ['Saved\\s+memories', 'evidence', 'saved instruction\\s+suggestions'].join('.*'),
          'i'
        )
      )
    ).toBeNull()
    expect(
      screen.queryByText(new RegExp(['Saved\\s+memories', 'proof'].join(',\\s+'), 'i'))
    ).toBeNull()
  })

  test('turns missing brief and result files into next steps', () => {
    render(
      <DescriptionTab
        task={{
          ...mockTask,
          state: 'completed',
          params: { ...mockTask.params, message: '' },
        }}
      />
    )

    expect(
      screen.getByText(
        'No brief was saved. Open Updates to see what was asked before accepting, retrying, or closing this task.'
      )
    ).toBeDefined()
    expect(
      screen.getByText(
        'No result files were saved. Use Next action above, then retry or create a follow-up task if files are still needed.'
      )
    ).toBeDefined()
    expect(screen.queryByText('No description provided.')).toBeNull()
    expect(screen.queryByText('No result files were attached.')).toBeNull()
  })

  test('summarizes failed task errors without raw service details', () => {
    render(
      <DescriptionTab
        task={{
          ...mockTask,
          state: 'failed',
          error: 'Rate limit exceeded: 429 from provider',
        }}
      />
    )

    expect(screen.getAllByText('Review recovery').length).toBeGreaterThan(1)
    expect(screen.queryByText('Triage failure')).toBeNull()
    expect(screen.queryByText('Failed')).toBeNull()
    expect(screen.getAllByText(/AI service is busy/i).length).toBeGreaterThan(0)
    expect(
      screen.getAllByText(/Wait a minute, then open details and retry/i).length
    ).toBeGreaterThan(0)
    expect(screen.queryByText(/when ready/i)).toBeNull()
    expect(screen.queryByText(/model service is busy/i)).toBeNull()
    expect(screen.queryByText(/429/)).toBeNull()
    expect(screen.queryByText(/provider/i)).toBeNull()
  })

  test('turns canceled task state into a decision step', () => {
    render(<DescriptionTab task={{ ...mockTask, state: 'canceled' }} />)

    expect(screen.getByText('Decide whether to continue')).toBeDefined()
    expect(
      screen.getByText('Create a new task or reopen the brief if this work still matters.')
    ).toBeDefined()
    expect(screen.queryByText('No current work')).toBeNull()
  })

  test('summarizes blocked task reasons without raw codes', () => {
    render(
      <DescriptionTab
        task={{
          ...mockTask,
          state: 'blocked',
          blockedReason: 'quota_exceeded',
          error: 'quota_exceeded: docker socket denied secret token abc',
        }}
      />
    )

    expect(screen.getByTestId('task-next-action').textContent).toContain(
      'Pause lower-priority work or ask an owner'
    )
    expect(screen.queryByText(/Free capacity/i)).toBeNull()
    expect(screen.queryByText(/quota_exceeded/i)).toBeNull()
    expect(screen.queryByText(/docker socket/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
  })

  test('summarizes blocked assignment hints without exposing service access details', () => {
    render(
      <DescriptionTab
        task={{
          ...mockTask,
          state: 'blocked',
          blockedReason: 'waiting_input',
          blockedHint: 'Needs API token secret for registry access',
          error: 'registry auth failed with token secret',
        }}
      />
    )

    expect(screen.getByTestId('task-assignment-blocked-guidance').textContent).toContain(
      'Waiting for account access'
    )
    expect(screen.getAllByText(/Waiting for account access/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/API token secret/i)).toBeNull()
    expect(screen.queryByText(/registry auth/i)).toBeNull()
  })

  test('labels unknown assignment state without exposing raw codes', () => {
    render(<DescriptionTab task={{ ...mockTask, state: 'waiting_for_agent' as never }} />)

    expect(screen.getByText('Check task status')).toBeDefined()
    expect(screen.queryByText(/waiting_for_agent/i)).toBeNull()
    expect(screen.queryByText(/waiting for agent/i)).toBeNull()
  })
})
