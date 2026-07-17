import { describe, expect, test } from 'vitest'
import {
  assignmentSummary,
  missingBriefCopy,
  nextActionForTask,
  taskHasBrief,
} from '@app/features/detail/model/taskGuidance'
import type { TaskSummary } from '@app/shared/api/orchestration'

const baseTask: TaskSummary = {
  id: 'task-1',
  state: 'backlog',
  method: 'tasks/send',
  params: { task: 'Review onboarding copy', message: 'Make the first run easier.' },
  priority: 'normal',
  progress: 0,
  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
  attempt: 1,
}

function task(overrides: Partial<TaskSummary> = {}): TaskSummary {
  return { ...baseTask, ...overrides }
}

describe('task guidance', () => {
  test('summarizes missing, loading, completed, and failed assignments', () => {
    expect(assignmentSummary(baseTask)).toEqual({
      label: 'Needs agent',
      detail: 'Choose an agent before this task can start.',
      hasAgent: false,
    })
    expect(assignmentSummary(task({ assignedTo: 'agent-1' }))).toMatchObject({
      label: 'Loading agent name',
      detail:
        'An agent was chosen, but its name has not loaded yet. Open this task again so you can confirm the right agent before sending it.',
      hasAgent: true,
    })
    expect(
      assignmentSummary(task({ state: 'completed', assignedAgentName: 'Review Agent' })).detail
    ).toBe('This agent finished this task. Check the result before accepting it.')
    expect(
      assignmentSummary(task({ state: 'failed', assignedAgentName: 'Review Agent' })).detail
    ).toBe('This agent tried this task. Check retry steps before trying again.')
  })

  test('distinguishes a missing brief before and after dispatch', () => {
    const blankBacklog = task({ params: { task: 'Title only', message: ' ' } })
    const blankCompleted = task({
      state: 'completed',
      params: { task: 'Title only', message: '' },
    })

    expect(taskHasBrief(blankBacklog)).toBe(false)
    expect(missingBriefCopy(blankBacklog)).toContain('Only the task title was saved.')
    expect(missingBriefCopy(blankCompleted)).toContain('No brief was saved.')
  })

  test('guides every backlog assignment and brief combination', () => {
    expect(nextActionForTask(baseTask, 0, 0)).toMatchObject({
      title: 'Assign an agent',
      detail: 'Choose an agent, check the suggested saved notes and guidance, then send the task.',
    })
    expect(nextActionForTask(task({ assignedTo: 'agent-1' }), 0, 0)).toMatchObject({
      title: 'Ready to send',
      detail: 'Check the brief, then send it to this agent.',
    })
    expect(
      nextActionForTask(
        task({ assignedTo: 'agent-1', params: { task: 'Title only', message: '' } }),
        0,
        0
      )
    ).toMatchObject({ title: 'Add details before sending', tone: 'warn' })
    expect(
      nextActionForTask(task({ params: { task: 'Title only', message: ' ' } }), 0, 0)
    ).toMatchObject({ title: 'Add details and choose an agent', tone: 'warn' })
  })

  test('distinguishes waiting tasks with and without an agent', () => {
    expect(nextActionForTask(task({ state: 'queued', assignedTo: 'agent-1' }), 0, 0)).toEqual({
      title: 'Waiting for the agent to start',
      detail:
        'If this stays here, open Updates to check the last activity, then choose another agent if needed.',
      tone: 'default',
    })
    expect(nextActionForTask(task({ state: 'queued' }), 0, 0)).toEqual({
      title: 'Waiting for an agent',
      detail:
        'If this stays here, choose or start an agent so the task has someone to begin the work.',
      tone: 'warn',
    })
  })

  test('changes working guidance near completion', () => {
    expect(nextActionForTask(task({ state: 'working', progress: 20 }), 0, 0).detail).toContain(
      'Watch progress'
    )
    expect(nextActionForTask(task({ state: 'working', progress: 80 }), 0, 0).detail).toContain(
      'Prepare to check result files'
    )
  })

  test('keeps blocked and failed guidance free of raw service details', () => {
    const blocked = nextActionForTask(
      task({
        state: 'blocked',
        blockedReason: 'quota_exceeded',
        error: 'quota_exceeded: docker socket denied secret token abc',
      }),
      0,
      0
    )
    const failed = nextActionForTask(
      task({ state: 'failed', error: 'Rate limit exceeded: 429 from provider' }),
      0,
      0
    )

    expect(blocked.detail).toContain('Pause lower-priority work or ask an owner')
    expect(blocked.detail).not.toMatch(/quota_exceeded|docker socket|secret token/i)
    expect(failed).toMatchObject({ title: 'Check retry steps', tone: 'warn' })
    expect(failed.detail).toContain('AI service is busy')
    expect(failed.detail).not.toMatch(/429|provider/i)
  })

  test('guides completed tasks from their available handoff evidence', () => {
    const completed = task({ state: 'completed' })

    expect(nextActionForTask(completed, 1, 0).detail).toContain('Open result files')
    expect(nextActionForTask(completed, 0, 1).detail).toContain('Check what the agent reused')
    expect(nextActionForTask(completed, 0, 0).detail).toContain('Confirm the outcome')
  })

  test('turns canceled and unrecognized states into safe next steps', () => {
    expect(nextActionForTask(task({ state: 'canceled' }), 0, 0)).toMatchObject({
      title: 'Decide whether to continue',
      detail: 'Create a new task or reopen the brief if this work still matters.',
    })
    expect(nextActionForTask(task({ state: 'waiting_for_agent' as never }), 0, 0)).toMatchObject({
      title: 'Check current status',
      detail: 'Open Updates to check the latest activity before starting, retrying, or closing.',
    })
  })
})
