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
    expect(message).toContain('app could not reach the service')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear export conflict recovery step', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('exportAudit', new Error('409 conflict')),
      'The audit data changed while export was running. Refresh the audit view, then export again.'
    )
  })

  test('turns service failures into an audit setup recovery step', () => {
    const message = governanceAuditErrorMessage('loadAudit', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Governance audit is temporarily unavailable. Refresh the audit view, then try again. If it still fails, ask an owner or admin to check governance audit setup.'
    )
    expect(message).not.toContain('backend')
  })

  test('turns validation details into a time range next step', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('loadAudit', {
        error: 'Invalid time range',
      }),
      'Choose a valid time range, then apply the audit filters again.'
    )
  })

  test('turns reference validation details into a support-reference next step', () => {
    const message = governanceAuditErrorMessage('loadAudit', {
      error: 'Invalid scope id',
    })

    expectBeginnerMessage(
      message,
      'Check the selected organization, workspace, user, or task support reference, then apply the audit filters again.'
    )
    expect(message).not.toMatch(/task I[D]/)
  })
})
