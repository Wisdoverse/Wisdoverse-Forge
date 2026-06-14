import { describe, expect, test } from 'vitest'
import { agentTasksErrorMessage } from '@app/features/agents/model/taskErrorMessage'

describe('agentTasksErrorMessage', () => {
  test('maps permission failures to task queue access guidance', () => {
    expect(agentTasksErrorMessage(new Error('HTTP 403'))).toBe(
      "Ask an owner or admin to give you access to this agent's work list."
    )
  })

  test('maps structured permission failures without leaking policy details', () => {
    const message = agentTasksErrorMessage({
      detail: 'Forbidden: task queue policy denied',
      status: '403',
    })

    expect(message).toBe(
      "Ask an owner or admin to give you access to this agent's work list."
    )
    expect(message).not.toContain('policy denied')
  })

  test('maps structured rate limits to a wait and retry step', () => {
    const message = agentTasksErrorMessage({
      error: 'too many task query requests',
      statusCode: 429,
    })

    expect(message).toBe(
      "Too many task requests are happening right now. Wait a minute, then refresh this agent to load its work list."
    )
    expect(message).not.toContain('task query requests')
  })

  test('maps server error rate limits without leaking raw queue details', () => {
    const message = agentTasksErrorMessage({
      serverError: 'too many task query requests',
      statusCode: 429,
    })

    expect(message).toBe(
      "Too many task requests are happening right now. Wait a minute, then refresh this agent to load its work list."
    )
    expect(message).not.toContain('task query requests')
  })

  test('maps service failures without exposing transport details', () => {
    const message = agentTasksErrorMessage('Server error (503)')

    expect(message).toBe(
      "Refresh this agent to load its work list. If it still fails, ask an owner or admin to check this agent's task setup."
    )
    expect(message).not.toContain('503')
    expect(message).not.toContain('platform')
  })

  test('maps structured service failures without raw setup details', () => {
    const message = agentTasksErrorMessage({
      message: 'task backend database timeout',
      code: '503',
    })

    expect(message).toBe(
      "Refresh this agent to load its work list. If it still fails, ask an owner or admin to check this agent's task setup."
    )
    expect(message).not.toContain('database timeout')
  })

  test('maps network failures to retryable guidance', () => {
    const message = agentTasksErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Check your connection, then refresh this agent to load its work list.'
    )
    expect(message).not.toContain('Failed to fetch')
  })
})
