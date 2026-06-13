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
      'Sign in again, then retry this review action.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = approvalQueueErrorMessage('loadQueue', new TypeError('Failed to fetch'))

    expect(message).toContain('saved item review list could not load')
    expect(message).toContain('Forge could not connect while loading saved items')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('gives a clear conflict recovery step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('approveCandidate', new Error('409 conflict')),
      'This item changed while you were reviewing it. Refresh the list, then open it again.'
    )
  })

  test('turns service failures into saved item setup recovery', () => {
    const message = approvalQueueErrorMessage('loadQueue', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'The saved item review list could not load. Refresh the list so you see the latest items. Forge could not load saved items right now. Refresh the list, then try again. If it still fails, ask an owner or admin to check saved item setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('keeps permission guidance in saved note wording', () => {
    const message = approvalQueueErrorMessage('rejectCandidate', new Error('403 Forbidden'))

    expect(message).toContain('approve saved notes and instructions')
    expect(message).not.toContain('403 Forbidden')
    expect(message).not.toContain(['saved', 'memories'].join(' '))
  })

  test('turns validation details into a scope next step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('approveCandidate', {
        error: 'Scope ID is required',
      }),
      'Choose who can reuse it and review the original task preview, then try again.'
    )
  })
})
