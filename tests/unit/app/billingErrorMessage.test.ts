import { describe, expect, test } from 'vitest'
import { billingErrorMessage } from '@app/shared/model/billing.store'

describe('billingErrorMessage', () => {
  test('turns permission failures into a billing access next step', () => {
    expect(billingErrorMessage({ statusCode: 403 }, 'subscription')).toBe(
      'Plan and payment could not be loaded. Ask an owner or billing administrator for access.'
    )
  })

  test('turns server failures into an administrator recovery step', () => {
    expect(billingErrorMessage('HTTP 500', 'invoices')).toBe(
      'Invoices could not be loaded. The billing service is temporarily unavailable. Ask an administrator to check billing, then refresh this page.'
    )
  })

  test('turns network failures into a connection step', () => {
    expect(billingErrorMessage(new TypeError('Failed to fetch'), 'usage')).toBe(
      'Usage could not be loaded. The app could not reach the service. Check your connection, then refresh this page.'
    )
  })

  test('uses a safe fallback without exposing raw details', () => {
    expect(billingErrorMessage(new Error('database timeout on shard 7'), 'invoices')).toBe(
      'Invoices could not be loaded. Refresh this page. If it still fails, ask an administrator to check billing.'
    )
  })
})
