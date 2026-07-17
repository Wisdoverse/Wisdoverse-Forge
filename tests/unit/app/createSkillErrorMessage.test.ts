import { describe, expect, test } from 'vitest'
import { createSkillErrorMessage } from '@app/features/skills/model/createSkillErrorMessage'

describe('createSkillErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
  }

  test('normalizes existing beginner guidance from the skills store to the save button', () => {
    const message =
      'Check your connection, then save the skill again. Forge could not connect while saving it.'

    expect(createSkillErrorMessage(new Error(message))).toBe(
      'Check your connection, then choose Save skill again. Forge could not connect while saving it.'
    )
  })

  test('normalizes older failure-first beginner guidance from the skills store', () => {
    const message =
      'Forge could not connect while creating this instruction. Check your connection, then try again.'

    expectBeginnerMessage(
      createSkillErrorMessage(new Error(message)),
      'Check your connection, then choose Save skill again. Forge could not connect while saving it.'
    )
  })

  test('removes generic details from existing permission guidance', () => {
    const message =
      'You do not have permission to create workspace instructions. Ask an owner or admin to let you create saved instructions. Code: 403. Details: Forbidden'

    expectBeginnerMessage(
      createSkillErrorMessage(new Error(message)),
      'Ask an owner or admin to update your Skills access for this team space, then choose Save skill again.'
    )
    expect(createSkillErrorMessage(new Error(message))).not.toContain('workspace instructions')
    expect(createSkillErrorMessage(new Error(message))).not.toContain('Saved instructions')
  })

  test('turns raw network failures into recovery guidance', () => {
    const message = createSkillErrorMessage(new Error('Failed to fetch'))

    expect(message).toContain('Check your connection')
    expect(message).toContain('choose Save skill again')
    expect(message).not.toContain('save the skill again')
    expect(message).not.toContain('create the instruction again')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
    expect(message).not.toContain('Forge could not connect while creating this instruction')
  })

  test('maps raw permission failures without exposing API text', () => {
    const message = createSkillErrorMessage(new Error('API 403: Forbidden'))

    expect(message).toContain('Ask an owner or admin')
    expect(message).toContain('Skills access for this team space')
    expect(message).toContain('choose Save skill again')
    expect(message).not.toContain('create the instruction again')
    expect(message).not.toContain('workspace instructions')
    expect(message).not.toContain('Saved instructions')
    expect(message).not.toContain('Code:')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
    expect(message).not.toContain('You do not have permission')
  })

  test('maps role-required failures to saved instruction access guidance', () => {
    const message = createSkillErrorMessage('owner role required')

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to update your Skills access for this team space, then choose Save skill again.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('maps missing saved instruction routes to a page refresh step', () => {
    expectBeginnerMessage(
      createSkillErrorMessage(new Error('HTTP 404: Not Found')),
      'Open Skills again, then choose Save skill again.'
    )
  })

  test('treats status-code wording as missing saved-guidance recovery guidance', () => {
    const message = createSkillErrorMessage('GraphQL returned status code 404')

    expectBeginnerMessage(message, 'Open Skills again, then choose Save skill again.')
    expect(message).not.toContain('GraphQL')
    expect(message).not.toContain('status code')
  })

  test('maps duplicate saved instructions to a specific check step', () => {
    const message = createSkillErrorMessage(new Error('HTTP 409'))

    expectBeginnerMessage(
      message,
      'Open Skills to check for a similar item, then change the name or matching words and choose Save skill again.'
    )
    expect(message).not.toContain('Review the existing instructions')
  })

  test('turns validation details into a field-specific next step', () => {
    const message = createSkillErrorMessage(new Error('HTTP 422: {"message":"trigger is invalid"}'))

    expectBeginnerMessage(message, 'Check the matching words, then choose Save skill again.')
    expect(message).not.toContain('HTTP 422')
    expect(message).not.toContain('trigger is invalid')
    expect(message).not.toContain('try again')
  })

  test('does not treat validation numbers as status codes', () => {
    const message = createSkillErrorMessage('guidance name must be between 1 and 100 characters')

    expectBeginnerMessage(message, 'Enter a guidance name, then choose Save skill again.')
    expect(message).not.toContain('100')
  })

  test('uses server error details for field-specific guidance', () => {
    const message = createSkillErrorMessage({
      serverError: 'trigger words are required',
    })

    expectBeginnerMessage(message, 'Check the matching words, then choose Save skill again.')
    expect(message).not.toContain('trigger words are required')
  })

  test('turns service failures into a skill setup recovery step', () => {
    const message = createSkillErrorMessage(new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Open Skills again, then choose Save skill again. If it still fails, ask your team to confirm Skills is available for this team space.'
    )
    expect(message).not.toContain('owner or admin')
    expect(message).not.toContain('access')
    expect(message).not.toContain('backend')
    expect(message).not.toContain('service is temporarily unavailable')
    expect(message).not.toContain('Forge could not create')
  })

  test('keeps unformatted service failures on the saved-instruction recovery path', () => {
    const message = createSkillErrorMessage(new Error('database unavailable while saving skill'))

    expectBeginnerMessage(
      message,
      'Open Skills again, then choose Save skill again. If it still fails, ask your team to confirm Skills is available for this team space.'
    )
    expect(message).not.toContain('Check the instruction name')
    expect(message).not.toContain('owner or admin')
    expect(message).not.toContain('access')
    expect(message).not.toContain('database unavailable')
  })

  test('turns busy save failures into a plain wait step', () => {
    const message = createSkillErrorMessage(new Error('HTTP 429'))

    expectBeginnerMessage(
      message,
      'Wait a moment, then choose Save skill again. Forge is busy with skills right now.'
    )
    expect(message).not.toContain('Instruction setup')
  })

  test('turns unformatted rate-limit save failures into a plain wait step', () => {
    const message = createSkillErrorMessage('Rate limit exceeded by provider raw payload')

    expectBeginnerMessage(
      message,
      'Wait a moment, then choose Save skill again. Forge is busy with skills right now.'
    )
    expect(message).not.toContain('Rate limit')
    expect(message).not.toContain('provider')
    expect(message).not.toContain('raw payload')
    expect(message).not.toContain('Check the instruction name')
  })

  test('uses a check step for unknown save failures', () => {
    const message = createSkillErrorMessage(new Error('HTTP 418'))

    expectBeginnerMessage(
      message,
      'Check the guidance name, matching words, and reusable guidance, then choose Save skill again.'
    )
    expect(message).not.toContain('Review the fields')
    expect(message).not.toContain('required fields')
  })
})
