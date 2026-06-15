import { describe, expect, test } from 'vitest'
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'

describe('taskFailureCopy', () => {
  test('turns failed task details into beginner-safe recovery copy', () => {
    const message = taskFailurePreview('panic: stack trace line 7')

    expect(message).toBe('Stopped before finishing. Open details to see what happened and retry.')
    expect(message).not.toContain('panic')
    expect(message).not.toContain('stack trace')
  })

  test('does not expose technical blocked hints', () => {
    const message = taskBlockedPreview({
      blockedHint: 'stdout: panic stack trace line 7 from docker socket',
      blockedReason: 'waiting_input',
    })

    expect(message).toBe(
      'This task needs help before it can continue. Open details, review the latest update, then retry or ask an owner for help.'
    )
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
})
