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

    expect(message).toContain('Refresh the audit view, then apply the filters again.')
    expect(message).toContain('If it still does not load, check your connection')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
    expect(message).not.toContain('Governance audit history could not load')
  })

  test('explains export network failures with the export recovery path', () => {
    const message = governanceAuditErrorMessage('exportAudit', 'Network Error')

    expect(message).toContain('Keep secrets hidden')
    expect(message).toContain('choose Export audit history again')
    expect(message).not.toContain('audit export did not finish')
  })

  test('gives a clear export conflict recovery step', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('exportAudit', new Error('409 conflict')),
      'Refresh the audit view, then export again because audit data changed while export was running.'
    )
  })

  test('turns service failures into an audit setup recovery step', () => {
    const message = governanceAuditErrorMessage('loadAudit', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh the audit view, then apply the filters again. If it still fails, ask an owner or admin to check governance audit setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('Forge could not load')
  })

  test('turns missing routes into a view and access recovery step', () => {
    const message = governanceAuditErrorMessage('loadAudit', { status: 404 })

    expectBeginnerMessage(
      message,
      'Open the Admin audit view again, then retry. If it still fails, ask an owner or admin to check workspace access.'
    )
    expect(message).not.toContain('route')
  })

  test('turns rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      governanceAuditErrorMessage('loadAudit', { code: '429' }),
      'Wait a moment, then try again. Audit history is handling too many requests right now.'
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
