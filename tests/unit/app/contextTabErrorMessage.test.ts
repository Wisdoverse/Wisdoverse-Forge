import { describe, expect, test } from 'vitest'
import { contextTabErrorMessage } from '@app/features/detail/model/contextTabErrorMessage'

describe('contextTabErrorMessage', () => {
  test('maps permission failures to task context access guidance', () => {
    expect(contextTabErrorMessage(new Error('HTTP 403'))).toBe(
      "Task context could not be loaded. Ask an owner or admin to give you access to this task's context."
    )
  })

  test('maps platform failures without exposing transport details', () => {
    const message = contextTabErrorMessage('Server error (503)')

    expect(message).toBe(
      'Task context could not be loaded. The platform is temporarily unavailable. Try again in a few minutes.'
    )
    expect(message).not.toContain('503')
  })

  test('maps network failures to retryable guidance', () => {
    expect(contextTabErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Task context could not be loaded. Check your connection, then try again.'
    )
  })
})
