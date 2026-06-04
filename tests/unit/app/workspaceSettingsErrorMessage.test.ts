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

  test('maps validation failures to beginner-safe create guidance', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', new Error('API 422'))

    expectBeginnerMessage(
      message,
      'The project was not created. Check the name and required fields, then try again.'
    )
  })

  test('maps duplicate create failures to a name change next step', () => {
    expectBeginnerMessage(
      workspaceSettingsErrorMessage('project', 'create', 'Code: 409 already exists'),
      'The project was not created. Use a different name, then try again.'
    )
  })

  test('turns server details into an owner recovery step', () => {
    expectBeginnerMessage(
      workspaceSettingsErrorMessage(
        'team',
        'load',
        new Error('API 503: {"message":"database unavailable"}')
      ),
      'Workspace teams could not be loaded. The workspace settings service is temporarily unavailable. Ask an owner or admin to check the backend, then refresh Settings.'
    )
  })

  test('maps network failures to retryable setup guidance', () => {
    expectBeginnerMessage(
      workspaceSettingsErrorMessage('project', 'load', new TypeError('Failed to fetch')),
      'Workspace projects could not be loaded. Check your connection, then try again.'
    )
  })
})
