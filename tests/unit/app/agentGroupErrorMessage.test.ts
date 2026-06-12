import { describe, expect, test } from 'vitest'
import { agentGroupErrorMessage } from '@app/features/agents/model/agentGroupErrorMessage'

describe('agentGroupErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(agentGroupErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Task queue was not created. Ask an owner or admin to let you create and manage task queues in this project.'
    )
  })

  test('turns structured permission failures into an owner or admin next step', () => {
    const message = agentGroupErrorMessage({
      statusCode: '403',
      detail: 'owner role required',
    })

    expect(message).toBe(
      'Task queue was not created. Ask an owner or admin to let you create and manage task queues in this project.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('explains naming conflicts without leaking raw API wording', () => {
    expect(agentGroupErrorMessage(new Error('API 409 lane conflict'))).toBe(
      'Task queue was not created. A queue with this name may already exist. Use a different name, then try again.'
    )
  })

  test('explains structured naming conflicts without leaking raw API wording', () => {
    const message = agentGroupErrorMessage({
      code: '409',
      reason: 'lane conflict',
    })

    expect(message).toBe(
      'Task queue was not created. A queue with this name may already exist. Use a different name, then try again.'
    )
    expect(message).not.toContain('lane conflict')
  })

  test('gives a connection recovery path for network failures', () => {
    const message = agentGroupErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Task queue was not created. Forge could not connect while creating the task queue. Check your connection, then try again.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a safe retry step for service failures', () => {
    const message = agentGroupErrorMessage(new Error('Server error 503: database unavailable'))

    expect(message).toBe(
      'Task queue was not created. Forge could not create the task queue right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check task queue setup.'
    )
    expect(message).not.toContain('Server error')
    expect(message).not.toContain('platform')
  })

  test('gives a safe retry step for structured service failures', () => {
    const message = agentGroupErrorMessage({
      status: '503',
      message: 'database unavailable',
    })

    expect(message).toBe(
      'Task queue was not created. Forge could not create the task queue right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check task queue setup.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
  })
})
