import { describe, expect, test } from 'vitest'
import { governanceAuditErrorMessage } from '@app/features/governance/governanceAuditErrorMessages'

describe('governanceAuditErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Detail:')
  }

  test('turns auth failures into a sign-in instruction', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('loadAudit', new Error('401 Unauthorized')),
      'Sign in again, then retry this audit action.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = governanceAuditErrorMessage('loadAudit', new TypeError('Failed to fetch'))

    expect(message).toContain('governance audit could not load')
    expect(message).toContain('browser could not reach the server')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear export conflict recovery step', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('exportAudit', new Error('409 conflict')),
      'The audit data changed while export was running. Refresh the audit view, then export again.'
    )
  })

  test('turns validation details into a time range next step', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('loadAudit', {
        error: 'Invalid time range',
      }),
      'Choose a valid time range, then apply the audit filters again.'
    )
  })
})
