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
      'Check your connection, then create the instruction again. Forge could not connect while creating it.'

    expect(createSkillErrorMessage(new Error(message))).toBe(message)
  })

  test('normalizes older failure-first beginner guidance from the skills store', () => {
    const message =
      'Forge could not connect while creating this instruction. Check your connection, then try again.'

    expectBeginnerMessage(
      createSkillErrorMessage(new Error(message)),
      'Check your connection, then create the instruction again. Forge could not connect while creating it.'
    )
  })

  test('removes generic details from existing permission guidance', () => {
    const message =
      'You do not have permission to create workspace instructions. Ask an owner or admin to let you create saved instructions. Code: 403. Details: Forbidden'

    expectBeginnerMessage(
      createSkillErrorMessage(new Error(message)),
      'Ask an owner or admin to let you create saved instructions. Your account cannot create workspace instructions yet.'
    )
  })

  test('turns raw network failures into recovery guidance', () => {
    const message = createSkillErrorMessage(new Error('Failed to fetch'))

    expect(message).toContain('Check your connection')
    expect(message).toContain('create the instruction again')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
    expect(message).not.toContain('Forge could not connect while creating this instruction')
  })

  test('maps raw permission failures without exposing API text', () => {
    const message = createSkillErrorMessage(new Error('API 403: Forbidden'))

    expect(message).toContain('Ask an owner or admin')
    expect(message).toContain('cannot create workspace instructions yet')
    expect(message).not.toContain('Code:')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
    expect(message).not.toContain('You do not have permission')
  })

  test('maps missing saved instruction routes to a page refresh step', () => {
    expectBeginnerMessage(
      createSkillErrorMessage(new Error('HTTP 404: Not Found')),
      'Open Saved instructions again, then create the instruction.'
    )
  })

  test('turns validation details into a field-specific next step', () => {
    const message = createSkillErrorMessage(new Error('HTTP 422: {"message":"trigger is invalid"}'))

    expectBeginnerMessage(message, 'Check the matching words, then try again.')
    expect(message).not.toContain('HTTP 422')
    expect(message).not.toContain('trigger is invalid')
  })

  test('uses server error details for field-specific guidance', () => {
    const message = createSkillErrorMessage({
      serverError: 'trigger words are required',
    })

    expectBeginnerMessage(message, 'Check the matching words, then try again.')
    expect(message).not.toContain('trigger words are required')
  })

  test('turns service failures into a skill setup recovery step', () => {
    const message = createSkillErrorMessage(new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh Saved instructions, then create the instruction again. If it still fails, ask an owner or admin to check instruction setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('service is temporarily unavailable')
    expect(message).not.toContain('Forge could not create')
  })
})
