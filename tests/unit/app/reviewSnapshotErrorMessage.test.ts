import { describe, expect, test } from 'vitest'
import { reviewSnapshotErrorMessage } from '@app/features/detail/model/reviewSnapshotErrorMessage'

describe('reviewSnapshotErrorMessage', () => {
  test('names the visible fix status check action in recovery steps', () => {
    expect(reviewSnapshotErrorMessage('approve', new Error('401 Unauthorized'))).toBe(
      'Sign in again, then choose Check fix status. Forge could not confirm your finish access.'
    )
    expect(reviewSnapshotErrorMessage('load', new Error('HTTP 404'))).toBe(
      'Open this task again from the Tasks page, then choose Check fix status. Forge could not find the fix check for this task.'
    )
    expect(reviewSnapshotErrorMessage('approve', new Error('required status check pending'))).toBe(
      'Wait for automated checks to finish, then choose Check fix status before finishing.'
    )
  })

  test('turns role-required finish checks into access guidance', () => {
    const message = reviewSnapshotErrorMessage('load', 'owner role required')

    expect(message).toBe(
      'Ask an owner or admin to check finish access for this code project, then choose Check fix status.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('turns raw service failures into finish-access recovery guidance', () => {
    const message = reviewSnapshotErrorMessage(
      'approve',
      new Error('database unavailable while approving review snapshot')
    )

    expect(message).toBe(
      'Choose Check fix status, confirm automated checks passed, then finish this fix again. The fix was not finished. If it still fails, ask an owner or admin to check finish access for this code project.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('approving review snapshot')
  })
})
