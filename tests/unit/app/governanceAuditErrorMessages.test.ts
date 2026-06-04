import { describe, expect, test } from 'vitest'
import { governanceAuditErrorMessage } from '@app/features/governance/governanceAuditErrorMessages'

describe('governanceAuditErrorMessage', () => {
  test('turns auth failures into a sign-in instruction', () => {
    expect(governanceAuditErrorMessage('loadAudit', new Error('401 Unauthorized'))).toBe(
      'Sign in again, then retry this audit action. Code: 401.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = governanceAuditErrorMessage('loadAudit', new TypeError('Failed to fetch'))

    expect(message).toContain('governance audit could not load')
    expect(message).toContain('browser could not reach the server')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear export conflict recovery step', () => {
    expect(governanceAuditErrorMessage('exportAudit', new Error('409 conflict'))).toBe(
      'The audit data changed while export was running. Refresh the audit view, then export again. Code: 409.'
    )
  })

  test('keeps short validation details after the operator instruction', () => {
    expect(
      governanceAuditErrorMessage('loadAudit', {
        error: 'Invalid time range',
      })
    ).toBe(
      'The governance audit could not load. Refresh after the API is healthy, then apply the filters again. Detail: Invalid time range'
    )
  })
})
