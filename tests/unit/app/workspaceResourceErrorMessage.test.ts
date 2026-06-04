import { describe, expect, test } from 'vitest'
import { workspaceResourceErrorMessage } from '@app/shared/lib/workspaceResourceErrorMessage'

describe('workspaceResourceErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
  }

  test('turns network failures into connection guidance', () => {
    const message = workspaceResourceErrorMessage('team', 'update', new Error('Failed to fetch'))

    expect(message).toContain('app could not reach the service')
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
  })

  test('maps project permission failures without raw API text', () => {
    const message = workspaceResourceErrorMessage('project', 'delete', new Error('API 403: Forbidden'))

    expectBeginnerMessage(
      message,
      'You do not have permission to delete this project. Ask an owner or admin to update your role.'
    )
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
  })

  test('turns team delete blockers into a move-projects step', () => {
    const message = workspaceResourceErrorMessage(
      'team',
      'delete',
      new Error('HTTP 422: {"message":"Move projects first."}')
    )

    expectBeginnerMessage(
      message,
      "Move or delete this team's projects first, then delete the team again."
    )
    expect(message).not.toContain('HTTP 422')
  })

  test('turns server failures into a workspace setup recovery step', () => {
    const message = workspaceResourceErrorMessage('project', 'update', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Workspace settings are temporarily unavailable. Refresh Settings, then try again. If it still fails, ask an owner or admin to check workspace setup.'
    )
    expect(message).not.toContain('HTTP 500')
  })
})
