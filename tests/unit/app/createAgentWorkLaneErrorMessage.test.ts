import { describe, expect, test } from 'vitest'
import { createAgentWorkLaneErrorMessage } from '@app/features/agents/model/createAgentWorkLaneErrorMessage'

describe('createAgentWorkLaneErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      "Work lane was not created. Ask a workspace owner or admin to let you manage this project's work lanes."
    )
  })

  test('turns duplicate lane failures into an existing lane step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('API 409: duplicate lane'))).toBe(
      'Work lane was not created. A default lane may already exist. Refresh the project, then choose the existing lane.'
    )
  })

  test('turns network failures into a connection retry path', () => {
    expect(createAgentWorkLaneErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Work lane was not created. Check your connection, then try again.'
    )
  })
})
