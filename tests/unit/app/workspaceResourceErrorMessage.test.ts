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
      'Check your connection, then open Settings, then Teams, and save the team again.'
    )
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
  })

  test('maps project permission failures without raw API text', () => {
    const message = workspaceResourceErrorMessage(
      'project',
      'delete',
      new Error('API 403: Forbidden')
    )

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to update your team space access, then open Settings, then Projects, and delete the project again. You do not have permission to delete this project.'
    )
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
    expect(message).not.toContain('role')
  })

  test('maps structured permission failures without raw API text', () => {
    const message = workspaceResourceErrorMessage('team', 'update', {
      statusCode: '403',
      detail: 'owner role required',
    })

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to update your team space access, then open Settings, then Teams, and save the team again. You do not have permission to save this team.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('role')
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
      "Open Settings, then Projects, delete this team's projects first, then delete the team again."
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
      "Go to Tasks, finish this project's tasks first, then delete the project again."
    )
    expect(message).not.toContain('Move tasks first')
  })

  test('turns generic delete blockers into dependency cleanup guidance', () => {
    const message = workspaceResourceErrorMessage('project', 'delete', {
      status: 422,
      reason: 'cannot delete',
    })

    expectBeginnerMessage(
      message,
      'Go to Agents and Tasks, check what is using this project, then delete the project again.'
    )
    expect(message).not.toContain('cannot delete')
    expect(message).not.toContain('depend')
  })

  test('turns generic team delete blockers into project and owner access guidance', () => {
    const message = workspaceResourceErrorMessage('team', 'delete', {
      status: 422,
      reason: 'cannot delete',
    })

    expectBeginnerMessage(
      message,
      'Open Settings, then Projects, check this team for projects, then delete the team again. If it still fails, ask an owner or admin to check team access.'
    )
    expect(message).not.toContain('cannot delete')
    expect(message).not.toContain('owns')
  })

  test('turns edit conflicts into a current-resource check step', () => {
    const message = workspaceResourceErrorMessage('project', 'update', {
      status: 409,
      detail: 'conflict',
    })

    expectBeginnerMessage(
      message,
      'Open Settings, then Projects, check the current project, then try again. This project changed while you were editing.'
    )
    expect(message).not.toContain('review the current')
  })

  test('turns busy settings responses into an action-first retry step', () => {
    expectBeginnerMessage(
      workspaceResourceErrorMessage('team', 'delete', { status: 429 }),
      'Wait a moment, then delete the team again. Settings is busy.'
    )
  })

  test('turns server failures into a concrete team and project recovery step', () => {
    const message = workspaceResourceErrorMessage('project', 'update', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Open Settings, then Projects, and save the project again. If it still fails, ask an owner or admin to check Projects in Settings.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('service')
    expect(message).not.toContain('workspace setup')
    expect(message).not.toContain('team space setup')
  })

  test('turns structured server failures into concrete team and project recovery', () => {
    const message = workspaceResourceErrorMessage('team', 'delete', {
      statusCode: '503',
      message: 'database unavailable',
    })

    expectBeginnerMessage(
      message,
      'Open Settings, then Teams, and delete the team again. If it still fails, ask an owner or admin to check Teams in Settings.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
    expect(message).not.toContain('workspace setup')
    expect(message).not.toContain('team space setup')
  })
})
