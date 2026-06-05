import { describe, expect, test } from 'vitest'
import { createSkillErrorMessage } from '@app/features/skills/model/createSkillErrorMessage'

describe('createSkillErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
  }

  test('preserves existing beginner guidance from the skills store', () => {
    const message =
      'Forge could not connect while creating this skill. Check your connection, then try again.'

    expect(createSkillErrorMessage(new Error(message))).toBe(message)
  })

  test('removes generic details from existing permission guidance', () => {
    const message =
      'You do not have permission to create workspace skills. Ask an owner or admin to let you create reusable skills. Code: 403. Details: Forbidden'

    expectBeginnerMessage(
      createSkillErrorMessage(new Error(message)),
      'You do not have permission to create workspace skills. Ask an owner or admin to let you create reusable skills.'
    )
  })

  test('turns raw network failures into recovery guidance', () => {
    const message = createSkillErrorMessage(new Error('Failed to fetch'))

    expect(message).toContain('Forge could not connect while creating this skill')
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('maps raw permission failures without exposing API text', () => {
    const message = createSkillErrorMessage(new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission to create workspace skills')
    expect(message).toContain('Ask an owner or admin')
    expect(message).not.toContain('Code:')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
  })

  test('turns validation details into a field-specific next step', () => {
    const message = createSkillErrorMessage(new Error('HTTP 422: {"message":"trigger is invalid"}'))

    expectBeginnerMessage(message, 'Check the matching words, then try again.')
    expect(message).not.toContain('HTTP 422')
    expect(message).not.toContain('trigger is invalid')
  })

  test('turns service failures into a skill setup recovery step', () => {
    const message = createSkillErrorMessage(new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Forge could not create the skill right now. Refresh Skills, then try again. If it still fails, ask an owner or admin to check skill setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('service is temporarily unavailable')
  })
})
