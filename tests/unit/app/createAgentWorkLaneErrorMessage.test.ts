import { describe, expect, test } from 'vitest'
import { createAgentWorkLaneErrorMessage } from '@app/features/agents/model/createAgentWorkLaneErrorMessage'

describe('createAgentWorkLaneErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Work lane was not created. Ask an owner or admin to let you create and manage work lanes in this project.'
    )
  })

  test('turns duplicate lane failures into an existing lane step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('API 409: duplicate lane'))).toBe(
      'Work lane was not created. A starter lane may already exist. Refresh the project, then choose the existing lane.'
    )
  })

  test('turns invalid lane setup into a project selection step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('HTTP 422: validation failed'))).toBe(
      'Work lane was not created. Choose a project first, then try again.'
    )
  })

  test('turns network failures into a connection retry path', () => {
    const message = createAgentWorkLaneErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Work lane was not created. Forge could not connect while creating the work lane. Check your connection, then try again.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns service failures into a safe retry and owner check', () => {
    const message = createAgentWorkLaneErrorMessage(new Error('HTTP 500: database unavailable'))

    expect(message).toBe(
      'Work lane was not created. Forge could not create the work lane right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check work lane setup.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('platform')
  })
})
