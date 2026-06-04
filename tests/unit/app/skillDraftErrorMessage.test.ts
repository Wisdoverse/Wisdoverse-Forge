import { describe, expect, test } from 'vitest'
import { skillDraftErrorMessage } from '@app/features/detail/model/skillDraftErrorMessage'

describe('skillDraftErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(skillDraftErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Skill was not published. Ask a workspace owner or admin to let you create workspace skills.'
    )
  })

  test('explains duplicate names without leaking raw API text', () => {
    expect(skillDraftErrorMessage(new Error('API 409: duplicate name'))).toBe(
      'Skill was not published. A skill with this name may already exist. Rename it, then publish again.'
    )
  })

  test('turns network failures into a publish retry path', () => {
    expect(skillDraftErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Skill was not published. Check your connection, then publish again.'
    )
  })
})
