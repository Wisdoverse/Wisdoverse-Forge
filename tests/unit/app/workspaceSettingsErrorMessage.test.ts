import { describe, expect, test } from 'vitest'
import { workspaceSettingsErrorMessage } from '@app/pages/settings/model/workspaceSettingsErrorMessage'

describe('workspaceSettingsErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Detail:')
  }

  test('maps permission failures to workspace access guidance', () => {
    expectBeginnerMessage(
      workspaceSettingsErrorMessage('team', 'load', new Error('HTTP 403')),
      'Ask an owner or admin to update your workspace access.'
    )
  })

  test('maps structured auth failures to a sign-in step', () => {
    expectBeginnerMessage(
      workspaceSettingsErrorMessage('team', 'load', { statusCode: '401' }),
      'Sign in again, then refresh Settings to load workspace teams.'
    )
  })

  test('maps validation failures to beginner-safe create guidance', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', new Error('API 422'))

    expectBeginnerMessage(message, 'Check the name and required fields, then try again.')
    expect(message).not.toContain('The project was not created')
  })

  test('uses structured validation details to name the field to fix', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', {
      status: 422,
      detail: 'name is required',
    })

    expectBeginnerMessage(message, 'Enter a project name, then try again.')
    expect(message).not.toContain('The project was not created')
    expect(message).not.toContain('name is required')
  })

  test('maps duplicate create failures to a name change next step', () => {
    expectBeginnerMessage(
      workspaceSettingsErrorMessage('project', 'create', 'Code: 409 already exists'),
      'Use a different name, then try again.'
    )
  })

  test('maps missing setup resources to team-space language', () => {
    const message = workspaceSettingsErrorMessage('team', 'load', new Error('HTTP 404'))

    expectBeginnerMessage(
      message,
      'Refresh Settings to load workspace teams. The team space, team, or project may have changed.'
    )
    expect(message).not.toContain('organization')
  })

  test('turns server details into an owner recovery step', () => {
    const message = workspaceSettingsErrorMessage(
      'team',
      'load',
      new Error('API 503: {"message":"database unavailable"}')
    )

    expectBeginnerMessage(
      message,
      'Refresh Settings to load workspace teams. If it still fails, ask an owner or admin to check workspace setup.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('service')
  })

  test('turns structured server details into an owner recovery step', () => {
    const message = workspaceSettingsErrorMessage('project', 'load', {
      status: 503,
      message: 'database unavailable',
    })

    expectBeginnerMessage(
      message,
      'Refresh Settings to load workspace projects. If it still fails, ask an owner or admin to check workspace setup.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
  })

  test('maps network failures to retryable setup guidance', () => {
    const message = workspaceSettingsErrorMessage(
      'project',
      'load',
      new TypeError('Failed to fetch')
    )

    expectBeginnerMessage(
      message,
      'Check your connection, then refresh Settings to load workspace projects.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('starts create network failures with the recovery step', () => {
    const message = workspaceSettingsErrorMessage(
      'project',
      'create',
      new TypeError('Failed to fetch')
    )

    expectBeginnerMessage(
      message,
      'Check your connection, then create this project again. Forge could not connect while creating it.'
    )
    expect(message).not.toContain('Failed to fetch')
  })
})
