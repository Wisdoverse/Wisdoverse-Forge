import { describe, expect, test } from 'vitest'
import { createAgentWorkLaneErrorMessage } from '@app/features/agents/model/createAgentWorkLaneErrorMessage'

describe('createAgentWorkLaneErrorMessage', () => {
  test('turns sign-in failures into a Create Agent retry step', () => {
    const message = createAgentWorkLaneErrorMessage(new Error('HTTP 401: Unauthorized'))

    expect(message).toBe(
      'Sign in again, reopen Create Agent, and set up where tasks wait again. The waiting place was not created.'
    )
    expect(message).not.toContain('New Agent')
    expect(message).not.toContain('Unauthorized')
    expect(message).not.toContain('task queue')
  })

  test('turns permission failures into an owner or admin next step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Ask an owner or admin to let you set up where tasks wait in this project. The waiting place was not created.'
    )
  })

  test('turns structured permission failures into an owner or admin next step', () => {
    const message = createAgentWorkLaneErrorMessage({
      statusCode: '403',
      detail: 'owner role required',
    })

    expect(message).toBe(
      'Ask an owner or admin to let you set up where tasks wait in this project. The waiting place was not created.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('task queue')
  })

  test('turns duplicate waiting-place failures into an existing-place step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('API 409: duplicate lane'))).toBe(
      'Refresh the project, then choose the existing waiting place. A starter waiting place may already exist.'
    )
  })

  test('turns structured duplicate failures into an existing-place step', () => {
    const message = createAgentWorkLaneErrorMessage({
      code: '409',
      reason: 'duplicate lane',
    })

    expect(message).toBe(
      'Refresh the project, then choose the existing waiting place. A starter waiting place may already exist.'
    )
    expect(message).not.toContain('duplicate lane')
    expect(message).not.toContain('task queue')
  })

  test('turns invalid waiting-place setup into a project selection step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('HTTP 422: validation failed'))).toBe(
      'Choose a project first, then set up where tasks wait again. The waiting place was not created.'
    )
  })

  test('turns network failures into a connection retry path', () => {
    const message = createAgentWorkLaneErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Check your connection, then set up where tasks wait again. Forge could not connect while creating the waiting place.'
    )
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('task queue')
  })

  test('turns service failures into a safe retry and owner check', () => {
    const message = createAgentWorkLaneErrorMessage(new Error('HTTP 500: database unavailable'))

    expect(message).toBe(
      'Wait a few minutes, then set up where tasks wait again. Forge could not create the waiting place right now. If it still fails, ask an owner or admin to check task routing setup.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('platform')
    expect(message).not.toContain('task queue')
  })

  test('turns structured service failures into a safe retry and owner check', () => {
    const message = createAgentWorkLaneErrorMessage({
      status: '503',
      message: 'database unavailable',
    })

    expect(message).toBe(
      'Wait a few minutes, then set up where tasks wait again. Forge could not create the waiting place right now. If it still fails, ask an owner or admin to check task routing setup.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
    expect(message).not.toContain('task queue')
  })
})
