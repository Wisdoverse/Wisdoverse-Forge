import { describe, expect, test } from 'vitest'
import { agentTasksErrorMessage } from '@app/features/agents/model/taskErrorMessage'

describe('agentTasksErrorMessage', () => {
  test('maps permission failures to work lane access guidance', () => {
    expect(agentTasksErrorMessage(new Error('HTTP 403'))).toBe(
      'This agent task list could not be loaded. Ask an owner or admin to give you access to this agent or its work lane.'
    )
  })

  test('maps platform failures without exposing transport details', () => {
    const message = agentTasksErrorMessage('Server error (503)')

    expect(message).toBe(
      'This agent task list could not be loaded. The platform is temporarily unavailable. Try again in a few minutes.'
    )
    expect(message).not.toContain('503')
  })

  test('maps network failures to retryable guidance', () => {
    expect(agentTasksErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'This agent task list could not be loaded. Check your connection, then try again.'
    )
  })
})
