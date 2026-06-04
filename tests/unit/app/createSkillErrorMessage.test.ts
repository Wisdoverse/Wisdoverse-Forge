import { describe, expect, test } from 'vitest'
import { createSkillErrorMessage } from '@app/features/skills/model/createSkillErrorMessage'

describe('createSkillErrorMessage', () => {
  test('preserves existing beginner guidance from the skills store', () => {
    const message =
      'The skill could not be created because the browser could not reach the server. Check your connection and try again.'

    expect(createSkillErrorMessage(new Error(message))).toBe(message)
  })

  test('removes generic details from existing permission guidance', () => {
    const message =
      'You do not have permission to create workspace skills. Ask an admin to update your role. Code: 403. Details: Forbidden'

    expect(createSkillErrorMessage(new Error(message))).toBe(
      'You do not have permission to create workspace skills. Ask an admin to update your role. Code: 403.'
    )
  })

  test('turns raw network failures into recovery guidance', () => {
    const message = createSkillErrorMessage(new Error('Failed to fetch'))

    expect(message).toContain('browser could not reach the server')
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
  })

  test('maps raw permission failures without exposing API text', () => {
    const message = createSkillErrorMessage(new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission to create workspace skills')
    expect(message).toContain('Ask an admin')
    expect(message).toContain('Code: 403.')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
  })

  test('keeps useful validation details after the operator action', () => {
    const message = createSkillErrorMessage(
      new Error('HTTP 422: {"message":"trigger is invalid"}')
    )

    expect(message).toContain('Check the skill name, trigger pattern, and content')
    expect(message).toContain('Code: 422.')
    expect(message).toContain('Details: trigger is invalid')
    expect(message).not.toContain('HTTP 422')
  })
})
