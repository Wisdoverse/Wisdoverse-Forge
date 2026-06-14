import { describe, expect, test } from 'vitest'
import { createAgentWorkLaneErrorMessage } from '@app/features/agents/model/createAgentWorkLaneErrorMessage'

describe('createAgentWorkLaneErrorMessage', () => {
  test('turns sign-in failures into a Create Agent retry step', () => {
    const message = createAgentWorkLaneErrorMessage(new Error('HTTP 401: Unauthorized'))

    expect(message).toBe(
      'Task queue was not created. Sign in again, reopen Create Agent, and try creating the queue again.'
    )
    expect(message).not.toContain('New Agent')
    expect(message).not.toContain('Unauthorized')
  })

  test('turns permission failures into an owner or admin next step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Task queue was not created. Ask an owner or admin to let you create and manage task queues in this project.'
    )
  })

  test('turns structured permission failures into an owner or admin next step', () => {
    const message = createAgentWorkLaneErrorMessage({
      statusCode: '403',
      detail: 'owner role required',
    })

    expect(message).toBe(
      'Task queue was not created. Ask an owner or admin to let you create and manage task queues in this project.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('turns duplicate queue failures into an existing queue step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('API 409: duplicate lane'))).toBe(
      'Task queue was not created. A starter queue may already exist. Refresh the project, then choose the existing queue.'
    )
  })

  test('turns structured duplicate failures into an existing queue step', () => {
    const message = createAgentWorkLaneErrorMessage({
      code: '409',
      reason: 'duplicate lane',
    })

    expect(message).toBe(
      'Task queue was not created. A starter queue may already exist. Refresh the project, then choose the existing queue.'
    )
    expect(message).not.toContain('duplicate lane')
  })

  test('turns invalid queue setup into a project selection step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('HTTP 422: validation failed'))).toBe(
      'Task queue was not created. Choose a project first, then try again.'
    )
  })

  test('turns network failures into a connection retry path', () => {
    const message = createAgentWorkLaneErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Task queue was not created. Forge could not connect while creating the task queue. Check your connection, then try again.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns service failures into a safe retry and owner check', () => {
    const message = createAgentWorkLaneErrorMessage(new Error('HTTP 500: database unavailable'))

    expect(message).toBe(
      'Task queue was not created. Forge could not create the task queue right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check task queue setup.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('platform')
  })

  test('turns structured service failures into a safe retry and owner check', () => {
    const message = createAgentWorkLaneErrorMessage({
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
