import { describe, expect, test } from 'vitest'
import { skillDraftErrorMessage } from '@app/features/detail/model/skillDraftErrorMessage'

describe('skillDraftErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(skillDraftErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Ask an owner or admin to let you create saved instructions, then save again. Saved instruction was not saved.'
    )
  })

  test('keeps store permission guidance when the modal remaps publish errors', () => {
    expect(
      skillDraftErrorMessage(
        new Error(
          'Ask an owner or admin to let you create saved instructions for this team space, then create the instruction again.'
        )
      )
    ).toBe(
      'Ask an owner or admin to let you create saved instructions, then save again. Saved instruction was not saved.'
    )
  })

  test('normalizes older workspace-instruction permission guidance', () => {
    const message = skillDraftErrorMessage(
      new Error(
        'Ask an owner or admin to let you create saved instructions. Your account cannot create workspace instructions yet.'
      )
    )

    expect(message).toBe(
      'Ask an owner or admin to let you create saved instructions, then save again. Saved instruction was not saved.'
    )
    expect(message).not.toContain('workspace instructions')
  })

  test('explains duplicate names without leaking raw API text', () => {
    expect(skillDraftErrorMessage(new Error('API 409: duplicate name'))).toBe(
      'Rename it, then save again. A saved instruction with this name may already exist. Saved instruction was not saved.'
    )
  })

  test('maps missing saved-instruction access to reopening the task', () => {
    const message = skillDraftErrorMessage(new Error('HTTP 404'))

    expect(message).toBe(
      'Open this task again, then save the instruction again. Saved instruction was not saved. Saved instruction access may have changed.'
    )
    expect(message).not.toContain('Refresh the task')
  })

  test('explains structured duplicate names without leaking raw API text', () => {
    const message = skillDraftErrorMessage({
      detail: 'duplicate saved instruction name',
      statusCode: 409,
    })

    expect(message).toBe(
      'Rename it, then save again. A saved instruction with this name may already exist. Saved instruction was not saved.'
    )
    expect(message).not.toContain('An instruction with this name')
    expect(message).not.toContain('duplicate saved instruction name')
  })

  test('turns structured validation failures into field guidance', () => {
    const message = skillDraftErrorMessage({
      code: '422',
      error: 'validation failed: trigger words empty',
    })

    expect(message).toBe(
      'Check the name, matching words, and reusable instructions, then save again. Saved instruction was not saved.'
    )
    expect(message).not.toContain('trigger words empty')
    expect(message).not.toContain('trigger words')
  })

  test('turns structured rate limits into a short wait step', () => {
    const message = skillDraftErrorMessage({
      message: 'too many publish attempts',
      status: 429,
    })

    expect(message).toBe(
      'Wait a minute, then save again. Too many instruction changes are happening right now. Saved instruction was not saved.'
    )
    expect(message).not.toContain('too many publish attempts')
  })

  test('turns network failures into a publish retry path', () => {
    const message = skillDraftErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Check your connection, then save again. Forge could not connect while saving this instruction.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns service failures into a safe publish retry path', () => {
    const message = skillDraftErrorMessage(new Error('HTTP 500'))

    expect(message).toBe(
      'Wait a few minutes, then save again. Forge could not save this instruction right now. If it still fails, ask an owner or admin to check Saved instructions access.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('service is temporarily unavailable')
  })

  test('turns unknown publish failures into a draft check step', () => {
    const message = skillDraftErrorMessage(new Error('unexpected failure'))

    expect(message).toBe(
      'Check the draft, then save again. Saved instruction was not saved. If it still fails, ask an owner or admin to check Saved instructions access.'
    )
    expect(message).not.toContain('Review the draft')
    expect(message).not.toContain('unexpected failure')
  })
})
