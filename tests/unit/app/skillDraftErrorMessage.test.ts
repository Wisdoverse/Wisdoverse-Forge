import { describe, expect, test } from 'vitest'
import { skillDraftErrorMessage } from '@app/features/detail/model/skillDraftErrorMessage'

describe('skillDraftErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(skillDraftErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Skill was not published. Ask an owner or admin to let you create reusable skills.'
    )
  })

  test('explains duplicate names without leaking raw API text', () => {
    expect(skillDraftErrorMessage(new Error('API 409: duplicate name'))).toBe(
      'Skill was not published. A skill with this name may already exist. Rename it, then publish again.'
    )
  })

  test('turns network failures into a publish retry path', () => {
    const message = skillDraftErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Skill was not published. Forge could not connect while publishing this skill. Check your connection, then publish again.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns service failures into a safe publish retry path', () => {
    const message = skillDraftErrorMessage(new Error('HTTP 500'))

    expect(message).toBe(
      'Skill was not published. Forge could not publish this skill right now. Wait a few minutes, then publish again. If it still fails, ask an owner or admin to check skill setup.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('service is temporarily unavailable')
  })
})
