import { describe, expect, test } from 'vitest'
import { billingErrorMessage } from '@app/shared/model/billing.store'

describe('billingErrorMessage', () => {
  test('turns permission failures into a billing access next step', () => {
    expect(billingErrorMessage({ statusCode: 403 }, 'subscription')).toBe(
      'Plan and payment could not be loaded. Ask an owner or admin to give you billing access.'
    )
  })

  test('turns structured sign-in failures into a sign-in step', () => {
    const message = billingErrorMessage(
      { code: '401', detail: 'unauthorized billing token expired' },
      'subscription'
    )

    expect(message).toBe('Plan and payment could not be loaded. Sign in again, then open Billing.')
    expect(message).not.toContain('billing token expired')
  })

  test('turns structured permission details into a billing access step', () => {
    const message = billingErrorMessage(
      { error: 'Forbidden: billing policy denied', status: '403' },
      'invoices'
    )

    expect(message).toBe(
      'Invoices could not be loaded. Ask an owner or admin to give you billing access.'
    )
    expect(message).not.toContain('policy denied')
  })

  test('turns structured rate limits into a wait and refresh step', () => {
    const message = billingErrorMessage(
      { serverError: 'too many billing provider calls', statusCode: 429 },
      'usage'
    )

    expect(message).toBe(
      'Usage could not be loaded. Billing is busy. Wait a minute, then refresh this page.'
    )
    expect(message).not.toContain('provider calls')
  })

  test('turns server failures into an owner or admin recovery step', () => {
    const message = billingErrorMessage('HTTP 500', 'invoices')

    expect(message).toBe(
      'Invoices could not be loaded. Forge could not load billing right now. Ask an owner or admin to check billing, then refresh this page.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns network failures into a connection step', () => {
    const message = billingErrorMessage(new TypeError('Failed to fetch'), 'usage')

    expect(message).toBe(
      'Usage could not be loaded. Forge could not connect while loading billing. Check your connection, then refresh this page.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('uses a safe fallback without exposing raw details', () => {
    expect(billingErrorMessage(new Error('database timeout on shard 7'), 'invoices')).toBe(
      'Invoices could not be loaded. Refresh this page. If it still fails, ask an owner or admin to check billing.'
    )
  })

  test('uses a safe fallback for unknown structured billing details', () => {
    const message = billingErrorMessage(
      { serverError: 'database timeout on billing shard 7', status: 418 },
      'invoices'
    )

    expect(message).toBe(
      'Invoices could not be loaded. Refresh this page. If it still fails, ask an owner or admin to check billing.'
    )
    expect(message).not.toContain('database timeout')
    expect(message).not.toContain('shard')
  })
})
