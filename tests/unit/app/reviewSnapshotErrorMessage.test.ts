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
})
