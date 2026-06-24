import { describe, expect, test } from 'vitest'
import { agentTasksErrorMessage } from '@app/features/agents/model/taskErrorMessage'

describe('agentTasksErrorMessage', () => {
  test('maps permission failures to task queue access guidance', () => {
    expect(agentTasksErrorMessage(new Error('HTTP 403'))).toBe(
      "Ask an owner or admin to give you access to this agent's task list."
    )
  })

  test('maps structured permission failures without leaking policy details', () => {
    const message = agentTasksErrorMessage({
      detail: 'Forbidden: task queue policy denied',
      status: '403',
    })

    expect(message).toBe("Ask an owner or admin to give you access to this agent's task list.")
    expect(message).not.toContain('work list')
    expect(message).not.toContain('policy denied')
  })

  test('maps role-required failures to agent work access guidance', () => {
    const message = agentTasksErrorMessage('owner role required')

    expect(message).toBe("Ask an owner or admin to give you access to this agent's task list.")
    expect(message).not.toContain('work list')
    expect(message).not.toContain('owner role required')
  })

  test('maps structured rate limits to a wait and retry step', () => {
    const message = agentTasksErrorMessage({
      error: 'too many task query requests',
      statusCode: 429,
    })

    expect(message).toBe(
      'Too many task requests are happening right now. Wait a minute, then open Agents and choose this agent again.'
    )
    expect(message).not.toContain('task query requests')
    expect(message).not.toContain('refresh this agent')
    expect(message).not.toContain('Open Work')
  })

  test('maps server error rate limits without leaking raw queue details', () => {
    const message = agentTasksErrorMessage({
      serverError: 'too many task query requests',
      statusCode: 429,
    })

    expect(message).toBe(
      'Too many task requests are happening right now. Wait a minute, then open Agents and choose this agent again.'
    )
    expect(message).not.toContain('task query requests')
    expect(message).not.toContain('Open Work')
  })

  test('maps service failures without exposing transport details', () => {
    const message = agentTasksErrorMessage('Server error (503)')

    expect(message).toBe(
      "Open Agents, choose this agent again, then open Tasks. If it still fails, ask an owner or admin to check this agent's task list."
    )
    expect(message).not.toContain('work list')
    expect(message).not.toContain('503')
    expect(message).not.toContain('platform')
    expect(message).not.toContain('Refresh this agent')
    expect(message).not.toContain('Open Work')
  })

  test('maps structured service failures without raw setup details', () => {
    const message = agentTasksErrorMessage({
      message: 'task backend database timeout',
      code: '503',
    })

    expect(message).toBe(
      "Open Agents, choose this agent again, then open Tasks. If it still fails, ask an owner or admin to check this agent's task list."
    )
    expect(message).not.toContain('work list')
    expect(message).not.toContain('database timeout')
    expect(message).not.toContain('Open Work')
  })

  test('maps network failures to retryable guidance', () => {
    const message = agentTasksErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe('Check your connection, then open Agents and choose this agent again.')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('refresh this agent')
    expect(message).not.toContain('Open Work')
  })

  test('maps missing agents to a navigable Work step', () => {
    const message = agentTasksErrorMessage({ status: 404 })

    expect(message).toBe(
      'Open Agents, choose this agent again, then open Tasks to load the task list. This agent may have changed or been removed.'
    )
    expect(message).not.toContain('work list')
    expect(message).not.toContain('Refresh this page')
    expect(message).not.toContain('Open Work')
  })
})
