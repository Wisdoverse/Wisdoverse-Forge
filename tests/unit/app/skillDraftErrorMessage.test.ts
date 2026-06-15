import { describe, expect, test } from 'vitest'
import { skillDraftErrorMessage } from '@app/features/detail/model/skillDraftErrorMessage'

describe('skillDraftErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(skillDraftErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Instruction was not published. Ask an owner or admin to let you create saved instructions.'
    )
  })

  test('keeps store permission guidance when the modal remaps publish errors', () => {
    expect(
      skillDraftErrorMessage(
        new Error(
          'Ask an owner or admin to let you create saved instructions for this team space.'
        )
      )
    ).toBe(
      'Instruction was not published. Ask an owner or admin to let you create saved instructions.'
    )
  })

  test('normalizes older workspace-instruction permission guidance', () => {
    const message = skillDraftErrorMessage(
      new Error(
        'Ask an owner or admin to let you create saved instructions. Your account cannot create workspace instructions yet.'
      )
    )

    expect(message).toBe(
      'Instruction was not published. Ask an owner or admin to let you create saved instructions.'
    )
    expect(message).not.toContain('workspace instructions')
  })

  test('explains duplicate names without leaking raw API text', () => {
    expect(skillDraftErrorMessage(new Error('API 409: duplicate name'))).toBe(
      'Instruction was not published. An instruction with this name may already exist. Rename it, then publish again.'
    )
  })

  test('explains structured duplicate names without leaking raw API text', () => {
    const message = skillDraftErrorMessage({
      detail: 'duplicate saved instruction name',
      statusCode: 409,
    })

    expect(message).toBe(
      'Instruction was not published. An instruction with this name may already exist. Rename it, then publish again.'
    )
    expect(message).not.toContain('duplicate saved instruction name')
  })

  test('turns structured validation failures into field guidance', () => {
    const message = skillDraftErrorMessage({
      code: '422',
      error: 'validation failed: trigger words empty',
    })

    expect(message).toBe(
      'Instruction was not published. Check the name, trigger words, and reusable instructions, then publish again.'
    )
    expect(message).not.toContain('trigger words empty')
  })

  test('turns structured rate limits into a short wait step', () => {
    const message = skillDraftErrorMessage({
      message: 'too many publish attempts',
      status: 429,
    })

    expect(message).toBe(
      'Instruction was not published. Too many instruction changes are happening right now. Wait a minute, then publish again.'
    )
    expect(message).not.toContain('too many publish attempts')
  })

  test('turns network failures into a publish retry path', () => {
    const message = skillDraftErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Instruction was not published. Forge could not connect while publishing this instruction. Check your connection, then publish again.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns service failures into a safe publish retry path', () => {
    const message = skillDraftErrorMessage(new Error('HTTP 500'))

    expect(message).toBe(
      'Instruction was not published. Forge could not publish this instruction right now. Wait a few minutes, then publish again. If it still fails, ask an owner or admin to check instruction setup.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('service is temporarily unavailable')
  })
})
