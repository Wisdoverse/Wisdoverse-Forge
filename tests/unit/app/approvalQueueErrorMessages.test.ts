import { describe, expect, test } from 'vitest'
import { approvalQueueErrorMessage } from '@app/features/context/approvalQueueErrorMessages'

describe('approvalQueueErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Detail:')
  }

  test('turns auth failures into a sign-in instruction', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('loadQueue', new Error('401 Unauthorized')),
      'Sign in again, then choose Check saved items again.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = approvalQueueErrorMessage('loadQueue', new TypeError('Failed to fetch'))

    expect(message).toContain('Check your connection, then choose Check saved items again')
    expect(message).toContain('Forge could not connect while loading saved notes and instructions')
    expect(message).not.toContain('refresh Saved items')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('explains saved review network failures with the retry first', () => {
    const message = approvalQueueErrorMessage(
      'approveCandidate',
      new TypeError('NetworkError when attempting to fetch resource')
    )

    expectBeginnerMessage(
      message,
      'Check your connection, then choose Save item again. Forge could not connect while saving your choice.'
    )
    expect(message).not.toContain('try this saved item action again')
    expect(message).not.toContain('NetworkError')
  })

  test('gives a clear conflict recovery step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('approveCandidate', new Error('409 conflict')),
      'Choose Check saved items again, then open this item. It changed while you were checking it.'
    )
  })

  test('gives a refresh step when the saved item is missing', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('rejectCandidate', new Error('404 not found')),
      'Choose Check saved items again so you see the latest saved items. This item was not found.'
    )
  })

  test('turns service failures into saved notes setup recovery', () => {
    const message = approvalQueueErrorMessage('loadQueue', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Choose Check saved items again so you see the latest saved items. Saved items could not load. If it still fails, ask an owner or admin to check Saved items access.'
    )
    expect(message).not.toContain('Refresh the list')
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('does not blame saved item settings for an unformatted service failure', () => {
    const message = approvalQueueErrorMessage(
      'approveCandidate',
      new Error('database unavailable while saving context candidate')
    )

    expectBeginnerMessage(
      message,
      'Wait a few minutes, then choose Save item again. The item was not saved. If it still fails, ask an owner or admin to check Saved items access.'
    )
    expect(message).not.toContain('Check who can reuse it')
    expect(message).not.toContain('database unavailable')
  })

  test('keeps permission guidance in saved note wording', () => {
    const message = approvalQueueErrorMessage('rejectCandidate', new Error('403 Forbidden'))

    expect(message).toContain('Ask an owner or admin')
    expect(message).toContain('save or skip saved notes and instructions')
    expect(message).toContain('then choose Do not save again')
    expect(message).not.toContain('403 Forbidden')
    expect(message).not.toContain(['saved', 'memories'].join(' '))
  })

  test('turns role-required failures into saved item access guidance', () => {
    const message = approvalQueueErrorMessage('approveCandidate', 'owner role required')

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to let you save or skip saved notes and instructions, then choose Save item again. You do not have permission right now.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('turns validation details into a scope next step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('approveCandidate', {
        error: 'Scope ID is required',
      }),
      'Choose who can reuse it and check the original task details, then choose Save item again.'
    )
  })

  test('maps nested validation details to a scope next step', () => {
    const message = approvalQueueErrorMessage('approveCandidate', {
      error: { message: 'Scope ID is required' },
    })

    expectBeginnerMessage(
      message,
      'Choose who can reuse it and check the original task details, then choose Save item again.'
    )
    expect(message).not.toContain('Scope ID is required')
  })

  test('turns load validation details into a refresh step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('loadQueue', {
        detail: 'Scope ID is required',
      }),
      'Choose Check saved items again, then check who can reuse the selected items. Saved items could not load.'
    )
  })

  test('turns rate limits into a wait step first', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('loadQueue', new Error('429 too many requests')),
      'Wait a moment, then choose Check saved items again. Saved items are busy.'
    )
  })

  test('turns confirmation validation into the matching review action', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('rejectCandidate', {
        detail: 'confirmation is required',
      }),
      'Complete the confirmation step, then choose Do not save again.'
    )
  })
})
