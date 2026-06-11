import { describe, expect, test } from 'vitest'
import { agentTasksErrorMessage } from '@app/features/agents/model/taskErrorMessage'

describe('agentTasksErrorMessage', () => {
  test('maps permission failures to task queue access guidance', () => {
    expect(agentTasksErrorMessage(new Error('HTTP 403'))).toBe(
      "This agent's work list could not be loaded. Ask an owner or admin to give you access to this agent or its task queue."
    )
  })

  test('maps service failures without exposing transport details', () => {
    const message = agentTasksErrorMessage('Server error (503)')

    expect(message).toBe(
      "This agent's work list could not be loaded. Forge could not load this work list right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check this agent's task setup."
    )
    expect(message).not.toContain('503')
    expect(message).not.toContain('platform')
  })

  test('maps network failures to retryable guidance', () => {
    const message = agentTasksErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      "This agent's work list could not be loaded. Forge could not connect while loading this work list. Check your connection, then try again."
    )
    expect(message).not.toContain('Failed to fetch')
  })
})
