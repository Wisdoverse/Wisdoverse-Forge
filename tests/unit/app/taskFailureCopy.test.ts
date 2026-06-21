import { describe, expect, test } from 'vitest'
import {
  isRawTaskFailureDetail,
  taskBlockedPreview,
  taskFailurePreview,
} from '@app/shared/lib/taskFailureCopy'

describe('taskFailureCopy', () => {
  test('turns failed task details into beginner-safe recovery copy', () => {
    const message = taskFailurePreview('panic: stack trace line 7')

    expect(message).toBe(
      'Stopped before finishing. Open the task details, check the latest update, then retry when ready.'
    )
    expect(message).not.toContain('Open details')
    expect(message).not.toContain('panic')
    expect(message).not.toContain('stack trace')
  })

  test('does not expose technical blocked hints', () => {
    const message = taskBlockedPreview({
      blockedHint: 'stdout: panic stack trace line 7 from docker socket',
      blockedReason: 'waiting_input',
    })

    expect(message).toBe(
      'This task needs help before it can continue. Open the task details, check the latest update, then retry or ask an owner for help.'
    )
    expect(message).not.toContain('Open details')
    expect(message).not.toContain('stdout')
    expect(message).not.toContain('panic')
    expect(message).not.toContain('stack trace')
    expect(message).not.toContain('docker socket')
  })

  test('keeps account access blocked hints as a direct next step', () => {
    const message = taskBlockedPreview({
      blockedHint: 'Missing token secret for git provider.',
      blockedReason: 'waiting_input',
    })

    expect(message).toBe(
      'Waiting for account access. Add or reconnect the required service access, then retry.'
    )
    expect(message).not.toContain('token')
    expect(message).not.toContain('secret')
  })

  test('turns sign-in failures into a direct reconnect step', () => {
    const message = taskFailurePreview('401 Unauthorized')

    expect(message).toBe('Reconnect sign-in or service access, then retry.')
    expect(message).not.toContain('needs attention')
    expect(message).not.toContain('401')
  })

  test('turns busy or rate-limit blocked hints into plain wait guidance', () => {
    const message = taskBlockedPreview({
      blockedHint: 'Rate limit exceeded: 429 from provider',
      blockedReason: 'waiting_input',
    })

    expect(message).toBe(
      'Too much work is running right now. Wait a bit, then retry or ask an owner for help.'
    )
    expect(message).not.toContain('workspace')
    expect(message).not.toContain('429')
    expect(message).not.toContain('provider')
  })

  test('turns waiting approval into a plain task-details choice', () => {
    const message = taskBlockedPreview({
      blockedReason: 'waiting_approval',
    })

    expect(message).toBe(
      'Open the task details, read what the agent needs, then choose Allow and continue.'
    )
    expect(message).not.toContain('approval request')
    expect(message).not.toContain('approve or decline')
    expect(message).not.toContain('Continue or Stop')
  })

  test('turns approval blocked hints into a plain task-details choice', () => {
    const message = taskBlockedPreview({
      blockedHint: 'Waiting for SSH approval',
      blockedReason: 'waiting_input',
    })

    expect(message).toBe(
      'Open the task details to see what needs confirmation, then choose Allow and continue when it is ready.'
    )
    expect(message).not.toContain('approval')
    expect(message).not.toContain('SSH')
    expect(message).not.toContain('Stop')
  })

  test('identifies raw failure details before they reach user-facing summaries', () => {
    expect(isRawTaskFailureDetail('command exited 1')).toBe(true)
    expect(isRawTaskFailureDetail('provider token rejected')).toBe(true)
    expect(isRawTaskFailureDetail('Repository access needs reconnecting')).toBe(false)
  })
})
