import { describe, expect, test } from 'vitest'
import { billingErrorMessage } from '@app/shared/model/billing.store'

describe('billingErrorMessage', () => {
  test('turns permission failures into a billing access next step', () => {
    expect(billingErrorMessage({ statusCode: 403 }, 'subscription')).toBe(
      'Plan and payment could not be loaded. Ask an owner or admin to give you billing access.'
    )
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
})
