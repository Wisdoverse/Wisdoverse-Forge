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
      'Workspace teams could not be loaded. Ask an owner or admin to update your workspace access.'
    )
  })

  test('maps structured auth failures to a sign-in step', () => {
    expectBeginnerMessage(
      workspaceSettingsErrorMessage('team', 'load', { statusCode: '401' }),
      'Workspace teams could not be loaded. Sign in again, then return to Settings.'
    )
  })

  test('maps validation failures to beginner-safe create guidance', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', new Error('API 422'))

    expectBeginnerMessage(
      message,
      'The project was not created. Check the name and required fields, then try again.'
    )
  })

  test('uses structured validation details to name the field to fix', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', {
      status: 422,
      detail: 'name is required',
    })

    expectBeginnerMessage(
      message,
      'The project was not created. Enter a project name, then try again.'
    )
    expect(message).not.toContain('name is required')
  })

  test('maps duplicate create failures to a name change next step', () => {
    expectBeginnerMessage(
      workspaceSettingsErrorMessage('project', 'create', 'Code: 409 already exists'),
      'The project was not created. Use a different name, then try again.'
    )
  })

  test('turns server details into an owner recovery step', () => {
    const message = workspaceSettingsErrorMessage(
      'team',
      'load',
      new Error('API 503: {"message":"database unavailable"}')
    )

    expectBeginnerMessage(
      message,
      'Workspace teams could not be loaded. Forge could not load workspace settings right now. Refresh Settings, then try again. If it still fails, ask an owner or admin to check workspace setup.'
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
      'Workspace projects could not be loaded. Forge could not load workspace settings right now. Refresh Settings, then try again. If it still fails, ask an owner or admin to check workspace setup.'
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
      'Workspace projects could not be loaded. Forge could not connect while loading workspace settings. Check your connection, then try again.'
    )
    expect(message).not.toContain('Failed to fetch')
  })
})
