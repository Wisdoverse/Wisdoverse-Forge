import { describe, expect, test } from 'vitest'
import { workspaceSettingsErrorMessage } from '@app/pages/settings/model/workspaceSettingsErrorMessage'

describe('workspaceSettingsErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Detail:')
  }

  test('names the exact Settings section instead of a combined Teams and Projects page', () => {
    const message = workspaceSettingsErrorMessage('project', 'load', new TypeError('Failed to fetch'))

    expect(message).toContain('open Settings, then Projects again')
    expect(message).not.toContain('Settings and Teams and Projects')
  })

  test('maps permission failures to team space access guidance', () => {
    const message = workspaceSettingsErrorMessage('team', 'load', new Error('HTTP 403'))

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to update your team space access, then open Settings, then Teams again. You do not have access to these team settings right now.'
    )
    expect(message).not.toContain('workspace access')
  })

  test('maps structured permission failures to team space access guidance', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', {
      statusCode: '403',
      detail: 'owner role required',
    })

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to update your team space access, then create this project again. You do not have access to these project settings right now.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('workspace access')
  })

  test('maps structured auth failures to a sign-in step without workspace wording', () => {
    const message = workspaceSettingsErrorMessage('team', 'load', { statusCode: '401' })

    expectBeginnerMessage(
      message,
      'Sign in again, then open Settings, then Teams again.'
    )
    expect(message).not.toContain('workspace teams')
  })

  test('maps validation failures to beginner-safe create guidance', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', new Error('API 422'))

    expectBeginnerMessage(
      message,
      'Check the project name, team, and code link. You can leave the code link blank, then create this project again.'
    )
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

  test('uses structured validation details to explain code link fixes', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', {
      status: 422,
      detail: 'repository_url must be an HTTPS URL',
    })

    expectBeginnerMessage(
      message,
      'Paste an https:// code link without account details, or leave the code link blank and add code access in Settings.'
    )
    expect(message).not.toContain('repository_url')
    expect(message).not.toContain('required fields')
  })

  test('uses structured validation details to explain account detail fixes', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', {
      status: 422,
      detail: 'repository url includes username or token',
    })

    expectBeginnerMessage(
      message,
      'Remove account details from the code link. Save code access in Settings instead, then create this project again.'
    )
    expect(message).not.toContain('username')
    expect(message).not.toContain('token')
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
      'Open Settings, then Teams again. The team space, team, or project may have changed.'
    )
    expect(message).not.toContain('organization')
    expect(message).not.toContain('workspace teams')
  })

  test('turns server details into an owner recovery step', () => {
    const message = workspaceSettingsErrorMessage(
      'team',
      'load',
      new Error('API 503: {"message":"database unavailable"}')
    )

    expectBeginnerMessage(
      message,
      'Open Settings, then Teams again. If it still fails, ask an owner or admin to check Teams in Settings.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('service')
    expect(message).not.toContain('team space setup')
    expect(message).not.toContain('workspace setup')
  })

  test('turns structured server details into an owner recovery step', () => {
    const message = workspaceSettingsErrorMessage('project', 'load', {
      status: 503,
      message: 'database unavailable',
    })

    expectBeginnerMessage(
      message,
      'Open Settings, then Projects again. If it still fails, ask an owner or admin to check Projects in Settings.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
    expect(message).not.toContain('team space setup')
    expect(message).not.toContain('workspace projects')
  })

  test('starts project creation server failures with the Settings path', () => {
    const message = workspaceSettingsErrorMessage('project', 'create', {
      status: 503,
      message: 'database unavailable',
    })

    expectBeginnerMessage(
      message,
      'Open Settings, then Projects again, choose the team, then create this project again. If it still fails, ask an owner or admin to check Projects in Settings.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('Refresh Settings')
  })

  test('maps network failures to retryable setup guidance', () => {
    const message = workspaceSettingsErrorMessage(
      'project',
      'load',
      new TypeError('Failed to fetch')
    )

    expectBeginnerMessage(
      message,
      'Check your connection, then open Settings, then Projects again.'
    )
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('workspace projects')
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
