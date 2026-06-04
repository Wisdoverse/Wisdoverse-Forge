import { describe, expect, test } from 'vitest'
import { runHistoryErrorMessage } from '@app/features/detail/model/runHistoryErrorMessage'

describe('runHistoryErrorMessage', () => {
  test('maps permission failures to task access guidance', () => {
    expect(runHistoryErrorMessage(new Error('HTTP 403'))).toBe(
      'Run attempts could not be loaded. Ask an owner or admin to give you access to this task.'
    )
  })

  test('maps platform failures without exposing transport details', () => {
    const message = runHistoryErrorMessage('Server error (503)')

    expect(message).toBe(
      'Run attempts could not be loaded. The platform is temporarily unavailable. Try again in a few minutes.'
    )
    expect(message).not.toContain('503')
  })

  test('maps network failures to retryable guidance', () => {
    expect(runHistoryErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Run attempts could not be loaded. Check your connection, then try again.'
    )
  })
})
