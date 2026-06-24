import { describe, expect, test } from 'vitest'
import {
  resourceMemberErrorMessage,
  resourceMemberSelectionLostMessage,
} from '@app/features/manage-members/model/resourceMemberErrorMessages'

describe('resourceMemberErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
  }

  test('turns network failures into reachable next steps', () => {
    const message = resourceMemberErrorMessage('load', 'Project', new Error('Failed to fetch'))

    expect(message).toBe('Check your connection, then open Members for this project.')
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
  })

  test('turns member update network failures into a retry step first', () => {
    const message = resourceMemberErrorMessage('add', 'Team', new Error('Network error'))

    expectBeginnerMessage(
      message,
      'Check your connection, then add the person again. Forge could not connect while updating people access.'
    )
    expect(message).not.toContain('Network error')
  })

  test('maps auth failures without exposing raw API text', () => {
    const message = resourceMemberErrorMessage(
      'load',
      'Team',
      new Error('API 401: {"message":"token expired"}')
    )

    expectBeginnerMessage(message, 'Sign in again, then open Members for this team.')
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
      'Ask an owner or admin to give you access to manage people here, then open Members for this team. You do not have permission right now.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('update what you can do')
  })

  test('maps nested permission failures without treating them as connection failures', () => {
    const message = resourceMemberErrorMessage('add', 'Project', {
      error: { message: 'owner role required' },
    })

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to give you access to manage people here, then open Members for this project. You do not have permission right now.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('Check your connection')
  })

  test('maps role-required failures to an owner or admin action', () => {
    const message = resourceMemberErrorMessage('add', 'Project', 'owner role required')

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to give you access to manage people here, then open Members for this project. You do not have permission right now.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('keeps lost selection messages safe even before modal error mapping', () => {
    expect(resourceMemberSelectionLostMessage('Team')).toBe(
      'This team is no longer selected. Close Members, choose the team again, then add or change people.'
    )
    expect(resourceMemberSelectionLostMessage('Project')).toBe(
      'This project is no longer selected. Close Members, choose the project again, then add or change people.'
    )
  })

  test('turns missing member lists into a clear Members step first', () => {
    const message = resourceMemberErrorMessage('load', 'Project', new Error('HTTP 404: Not Found'))

    expectBeginnerMessage(
      message,
      'Open Members for this project again, or choose another project. This project may have changed or been removed.'
    )
    expect(message).not.toContain('HTTP 404')
    expect(message).not.toContain('Not Found')
    expect(message).not.toContain('People for this project are not available')
  })

  test('turns member conflicts into a current access check and named retry step', () => {
    const message = resourceMemberErrorMessage(
      'updateRole',
      'Project',
      new Error('API 409: {"message":"role already changed"}')
    )

    expectBeginnerMessage(
      message,
      "Open Members for this project again, check who has access, then save the access change again. This person's access changed while you were editing."
    )
    expect(message).not.toContain('API 409')
    expect(message).not.toContain('role already changed')
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
      'Open Members for this team again. If it still fails, ask an owner or admin to check people access settings.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('service')
  })

  test('turns load validation details into a Members step', () => {
    const message = resourceMemberErrorMessage('load', 'Project', {
      status: 422,
      detail: 'member filter is invalid',
    })

    expectBeginnerMessage(message, 'Open Members for this project again.')
    expect(message).not.toContain('member filter')
  })

  test('turns structured service failures into a people access settings step', () => {
    const message = resourceMemberErrorMessage('remove', 'Team', {
      statusCode: '503',
      message: 'database unavailable',
    })

    expectBeginnerMessage(
      message,
      'Open Members for this team again, then remove the person again. Forge could not update people access right now. If it still fails, ask an owner or admin to check people access settings.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
  })

  test('keeps unformatted service failures on the people access recovery path', () => {
    const message = resourceMemberErrorMessage(
      'remove',
      'Team',
      new Error('database unavailable while removing team member')
    )

    expectBeginnerMessage(
      message,
      'Open Members for this team again, then remove the person again. Forge could not update people access right now. If it still fails, ask an owner or admin to check people access settings.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('last owner')
  })

  test('turns busy member updates into a wait step first', () => {
    const message = resourceMemberErrorMessage('updateRole', 'Project', {
      statusCode: '429',
      message: 'Too many requests',
    })

    expectBeginnerMessage(
      message,
      'Wait a moment, then save the access change again. People access is busy right now.'
    )
    expect(message).not.toContain('Too many requests')
  })

  test('turns unknown member failures into a Members step first', () => {
    const message = resourceMemberErrorMessage('remove', 'Project', {
      statusCode: '418',
      message: 'unexpected member state',
    })

    expectBeginnerMessage(
      message,
      'Open Members for this project again, then remove the person again. Forge could not remove this person from this project.'
    )
    expect(message).not.toContain('unexpected member state')
  })
})
