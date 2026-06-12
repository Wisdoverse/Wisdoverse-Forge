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
      'Your sign-in expired. Sign in again, then retry this audit action.'
    )
  })

  test('explains load network failures without exposing only a transport error', () => {
    const message = governanceAuditErrorMessage('loadAudit', new TypeError('Failed to fetch'))

    expect(message).toContain('Governance audit history could not load')
    expect(message).toContain('Forge could not connect while loading audit history')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
  })

  test('explains export network failures with the export recovery path', () => {
    const message = governanceAuditErrorMessage('exportAudit', 'Network Error')

    expect(message).toContain('audit export did not finish')
    expect(message).toContain('Keep secrets hidden')
    expect(message).toContain('Forge could not connect while exporting audit history')
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
      'Forge could not load governance audit history right now. Refresh the audit view, then try again. If it still fails, ask an owner or admin to check governance audit setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns missing routes into a view and access recovery step', () => {
    const message = governanceAuditErrorMessage('loadAudit', { status: 404 })

    expectBeginnerMessage(
      message,
      'Governance audit is not available from this view. Open the Admin audit view again, then retry. If it still fails, ask an owner or admin to check workspace access.'
    )
    expect(message).not.toContain('route')
  })

  test('turns rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('loadAudit', { code: '429' }),
      'Governance audit is handling too many requests right now. Wait a moment, then try again.'
    )
  })

  test('turns validation details into a time range next step', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('loadAudit', {
        error: 'Invalid time range',
      }),
      'Choose a valid time range. Make sure From is before To, then apply the audit filters again.'
    )
  })

  test('turns limit validation details into the allowed range', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('loadAudit', {
        error: 'limit must be less than or equal to 200',
      }),
      'Enter a record limit from 1 to 200, then apply the audit filters again.'
    )
  })

  test('turns reference validation details into a support-reference next step', () => {
    const message = governanceAuditErrorMessage('loadAudit', {
      error: 'Invalid scope id',
    })

    expectBeginnerMessage(
      message,
      'Check the selected team space, workspace, user, or task support reference, then apply the audit filters again.'
    )
    expect(message).not.toContain('selected organization')
    expect(message).not.toMatch(/task I[D]/)
  })
})
