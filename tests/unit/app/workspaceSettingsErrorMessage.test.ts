import { describe, expect, test } from 'vitest'
import { workspaceSettingsErrorMessage } from '@app/pages/settings/model/workspaceSettingsErrorMessage'

describe('workspaceSettingsErrorMessage', () => {
  test('maps permission failures to workspace access guidance', () => {
    expect(workspaceSettingsErrorMessage('team', 'load', new Error('HTTP 403'))).toBe(
      'Workspace teams could not be loaded. Ask an owner or admin to update your workspace access.'
    )
  })

  test('maps validation failures to beginner-safe create guidance', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', new Error('API 422'))

    expect(message).toBe(
      'The project was not created. Check the name and required fields, then try again.'
    )
    expect(message).not.toContain('API 422')
  })

  test('maps duplicate create failures to a name change next step', () => {
    expect(workspaceSettingsErrorMessage('project', 'create', 'Code: 409 already exists')).toBe(
      'The project was not created. Use a different name, then try again. Detail: already exists'
    )
  })

  test('keeps short server details after the operator instruction', () => {
    expect(
      workspaceSettingsErrorMessage(
        'team',
        'load',
        new Error('API 503: {"message":"database unavailable"}')
      )
    ).toBe(
      'Workspace teams could not be loaded. The workspace settings service had a server problem. Try again after the backend is healthy. Detail: database unavailable'
    )
  })

  test('maps network failures to retryable setup guidance', () => {
    expect(workspaceSettingsErrorMessage('project', 'load', new TypeError('Failed to fetch'))).toBe(
      'Workspace projects could not be loaded. Check your connection, then try again.'
    )
  })
})
