import { describe, expect, test } from 'vitest'
import { resourceMemberErrorMessage } from '@app/features/manage-members/model/resourceMemberErrorMessages'

describe('resourceMemberErrorMessage', () => {
  test('turns network failures into reachable next steps', () => {
    const message = resourceMemberErrorMessage('load', 'Project', new Error('Failed to fetch'))

    expect(message).toContain('browser could not reach the server')
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
  })

  test('maps auth failures without exposing raw API text', () => {
    const message = resourceMemberErrorMessage(
      'load',
      'Team',
      new Error('API 401: {"message":"token expired"}')
    )

    expect(message).toContain('Sign in again')
    expect(message).toContain('Code: 401.')
    expect(message).not.toContain('API 401')
  })

  test('maps permission failures to an owner or admin action', () => {
    const message = resourceMemberErrorMessage('add', 'Project', new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission')
    expect(message).toContain('Ask an owner or admin')
    expect(message).toContain('Code: 403.')
    expect(message).not.toContain('Forbidden')
  })

  test('keeps safe validation details after the beginner action', () => {
    const message = resourceMemberErrorMessage(
      'remove',
      'Project',
      new Error('API 422: {"message":"Choose a different owner first."}')
    )

    expect(message).toContain('last owner')
    expect(message).toContain('Code: 422.')
    expect(message).toContain('Details: Choose a different owner first.')
    expect(message).not.toContain('API 422')
  })
})
