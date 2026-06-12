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

    expect(message).toBe(
      'Team could not be saved. Forge could not connect while saving workspace settings. Check your connection, then try again.'
    )
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
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

  test('maps structured permission failures without raw API text', () => {
    const message = workspaceResourceErrorMessage('team', 'update', {
      statusCode: '403',
      detail: 'owner role required',
    })

    expectBeginnerMessage(
      message,
      'You do not have permission to save this team. Ask an owner or admin to update your role.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('uses structured validation details to name the field to fix', () => {
    const message = workspaceResourceErrorMessage('project', 'update', {
      status: '422',
      detail: 'project name is required',
    })

    expectBeginnerMessage(message, 'Enter a project name, then save again.')
    expect(message).not.toContain('project name is required')
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

  test('turns structured project delete blockers into a task cleanup step', () => {
    const message = workspaceResourceErrorMessage('project', 'delete', {
      status: 422,
      reason: 'Move tasks first.',
    })

    expectBeginnerMessage(
      message,
      "Move or finish this project's tasks first, then delete the project again."
    )
    expect(message).not.toContain('Move tasks first')
  })

  test('turns server failures into a workspace setup recovery step', () => {
    const message = workspaceResourceErrorMessage('project', 'update', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Forge could not save workspace settings right now. Refresh Settings, then save the project again. If it still fails, ask an owner or admin to check workspace setup.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('service')
  })

  test('turns structured server failures into workspace setup recovery', () => {
    const message = workspaceResourceErrorMessage('team', 'delete', {
      statusCode: '503',
      message: 'database unavailable',
    })

    expectBeginnerMessage(
      message,
      'Forge could not delete this team right now. Refresh Settings, then delete the team again. If it still fails, ask an owner or admin to check workspace setup.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
  })
})
