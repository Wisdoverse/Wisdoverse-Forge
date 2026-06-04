import { describe, expect, test } from 'vitest'
import { workspaceSettingsErrorMessage } from '@app/pages/settings/ui/workspaceSettingsErrors'

describe('workspaceSettingsErrorMessage', () => {
  test('maps team loading permission failures to an access recovery step', () => {
    expect(
      workspaceSettingsErrorMessage(
        'load-teams',
        new Error('API 403: {"error":"owner role required"}')
      )
    ).toBe(
      'You do not have permission to load teams. Ask an owner or admin to update your access. Code: 403. Details: owner role required'
    )
  })

  test('maps team creation validation failures to field guidance', () => {
    expect(
      workspaceSettingsErrorMessage(
        'create-team',
        new Error('API 422: {"message":"team name is required"}')
      )
    ).toBe('Check the team name, then try again. Code: 422. Details: team name is required')
  })

  test('maps project loading network failures to a connection recovery step', () => {
    expect(workspaceSettingsErrorMessage('load-projects', new TypeError('Failed to fetch'))).toBe(
      'Projects could not load because the browser could not reach the server. Check your connection, then try again.'
    )
  })

  test('maps project creation permission failures to the team admin next step', () => {
    expect(
      workspaceSettingsErrorMessage(
        'create-project',
        new Error('API 403: {"message":"project role required"}')
      )
    ).toBe(
      'You do not have permission to create projects in this team. Ask a team admin to update your project access. Code: 403. Details: project role required'
    )
  })
})
