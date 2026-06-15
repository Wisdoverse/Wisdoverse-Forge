import { describe, expect, test } from 'vitest'
import { resourceMemberErrorMessage } from '@app/features/manage-members/model/resourceMemberErrorMessages'

describe('resourceMemberErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
  }

  test('turns network failures into reachable next steps', () => {
    const message = resourceMemberErrorMessage('load', 'Project', new Error('Failed to fetch'))

    expect(message).toBe(
      'Check your connection, then reopen members for this project.'
    )
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
  })

  test('maps auth failures without exposing raw API text', () => {
    const message = resourceMemberErrorMessage(
      'load',
      'Team',
      new Error('API 401: {"message":"token expired"}')
    )

    expectBeginnerMessage(message, 'Sign in again, then reopen members for this team.')
    expect(message).not.toContain('API 401')
  })

  test('maps permission failures to an owner or admin action', () => {
    const message = resourceMemberErrorMessage('add', 'Project', new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission')
    expect(message).toContain('Ask an owner or admin')
    expect(message).not.toContain('Code:')
    expect(message).not.toContain('Forbidden')
  })

  test('maps structured permission failures without raw API text', () => {
    const message = resourceMemberErrorMessage('updateRole', 'Team', {
      statusCode: '403',
      detail: 'owner role required',
    })

    expectBeginnerMessage(
      message,
      'You do not have permission to manage people for this team. Ask an owner or admin to give you access to manage people here.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('update what you can do')
  })

  test('uses structured validation details to explain missing access choices', () => {
    const message = resourceMemberErrorMessage('add', 'Project', {
      status: '422',
      reason: 'role is required',
    })

    expectBeginnerMessage(message, 'Choose this person and what they can do, then add them again.')
    expect(message).not.toContain('role is required')
  })

  test('turns last-owner style remove failures into a clear owner step', () => {
    const message = resourceMemberErrorMessage(
      'remove',
      'Project',
      new Error('API 422: {"message":"Choose a different owner first."}')
    )

    expectBeginnerMessage(
      message,
      'Choose a different owner first, then remove this person from this project.'
    )
    expect(message).not.toContain('API 422')
  })

  test('turns structured last-owner failures into a clear owner step', () => {
    const message = resourceMemberErrorMessage('updateRole', 'Project', {
      status: 422,
      detail: 'Choose a different owner first.',
    })

    expectBeginnerMessage(
      message,
      'Choose a different owner first, then change what this person can do on this project.'
    )
    expect(message).not.toContain('Choose a different owner first.')
  })

  test('uses server error details for last-owner failures', () => {
    const message = resourceMemberErrorMessage('remove', 'Team', {
      statusCode: 422,
      serverError: 'owner must remain on team',
    })

    expectBeginnerMessage(
      message,
      'Choose a different owner first, then remove this person from this team.'
    )
    expect(message).not.toContain('owner must remain')
  })

  test('turns service failures into a people access settings step', () => {
    const message = resourceMemberErrorMessage('load', 'Team', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh members to load people for this team. If it still fails, ask an owner or admin to check people access settings.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('service')
  })

  test('turns load validation details into a refresh step', () => {
    const message = resourceMemberErrorMessage('load', 'Project', {
      status: 422,
      detail: 'member filter is invalid',
    })

    expectBeginnerMessage(message, 'Refresh members to load people for this project.')
    expect(message).not.toContain('member filter')
  })

  test('turns structured service failures into a people access settings step', () => {
    const message = resourceMemberErrorMessage('remove', 'Team', {
      statusCode: '503',
      message: 'database unavailable',
    })

    expectBeginnerMessage(
      message,
      'Forge could not update people access right now. Refresh members, then remove the person again. If it still fails, ask an owner or admin to check people access settings.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
  })
})
