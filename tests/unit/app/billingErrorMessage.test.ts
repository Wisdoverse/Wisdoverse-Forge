import { describe, expect, test } from 'vitest'
import {
  billingActionErrorMessage,
  billingErrorMessage,
} from '@app/features/billing/model/billing.store'

describe('billingErrorMessage', () => {
  test('turns permission failures into a billing access next step', () => {
    expect(billingErrorMessage({ statusCode: 403 }, 'subscription')).toBe(
      'Ask an owner or admin to give you billing access, then choose Check billing again to load plan and payment.'
    )
  })

  test('turns structured sign-in failures into a sign-in step', () => {
    const message = billingErrorMessage(
      { code: '401', detail: 'unauthorized billing token expired' },
      'subscription'
    )

    expect(message).toBe(
      'Sign in again, then open Billing and choose Check billing again to load plan and payment.'
    )
    expect(message).not.toContain('billing token expired')
  })

  test('turns structured permission details into a billing access step', () => {
    const message = billingErrorMessage(
      { error: 'Forbidden: billing policy denied', status: '403' },
      'invoices'
    )

    expect(message).toBe(
      'Ask an owner or admin to give you billing access, then choose Check billing again to load invoices.'
    )
    expect(message).not.toContain('policy denied')
  })

  test('turns nested permission details into a billing access step', () => {
    const message = billingErrorMessage(
      { error: { message: 'Forbidden: billing policy denied', statusCode: 403 } },
      'invoices'
    )

    expect(message).toBe(
      'Ask an owner or admin to give you billing access, then choose Check billing again to load invoices.'
    )
    expect(message).not.toContain('policy denied')
  })

  test('turns role-required details into a billing access step', () => {
    const message = billingErrorMessage('owner role required', 'usage')

    expect(message).toBe(
      'Ask an owner or admin to give you billing access, then choose Check billing again to load usage.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('turns structured rate limits into a wait and refresh step', () => {
    const message = billingErrorMessage(
      { serverError: 'too many billing provider calls', statusCode: 429 },
      'usage'
    )

    expect(message).toBe(
      'Wait a minute, then choose Check billing again to load usage. Billing is busy.'
    )
    expect(message).not.toContain('provider calls')
  })

  test('turns server failures into an owner or admin recovery step', () => {
    const message = billingErrorMessage('HTTP 500', 'invoices')

    expect(message).toBe(
      'Choose Check billing again to load invoices. If it still fails, ask an owner or admin to check billing.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns network failures into a connection step', () => {
    const message = billingErrorMessage(new TypeError('Failed to fetch'), 'usage')

    expect(message).toBe(
      'Check your connection, then choose Check billing again to load usage. Forge could not connect while loading billing.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('uses a safe fallback without exposing raw details', () => {
    expect(billingErrorMessage(new Error('database timeout on shard 7'), 'invoices')).toBe(
      'Choose Check billing again to load invoices. If it still fails, ask an owner or admin to check billing.'
    )
  })

  test('uses a safe fallback for unknown structured billing details', () => {
    const message = billingErrorMessage(
      { serverError: 'database timeout on billing shard 7', status: 418 },
      'invoices'
    )

    expect(message).toBe(
      'Choose Check billing again to load invoices. If it still fails, ask an owner or admin to check billing.'
    )
    expect(message).not.toContain('database timeout')
    expect(message).not.toContain('shard')
  })
})

describe('billingActionErrorMessage', () => {
  test('turns checkout network failures into a payment-page recovery step', () => {
    const message = billingActionErrorMessage(new TypeError('Failed to fetch'), 'checkout')

    expect(message).toBe(
      'Check your connection, then try opening the secure payment page again. Forge could not connect while opening billing.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns portal permission failures into an owner or admin step', () => {
    const message = billingActionErrorMessage(
      { statusCode: 403, detail: 'billing portal forbidden' },
      'portal'
    )

    expect(message).toBe(
      'Ask an owner or admin to give you billing access, then try opening the billing management page again.'
    )
    expect(message).not.toContain('forbidden')
  })

  test('turns role-required action failures into a billing access step', () => {
    const message = billingActionErrorMessage('owner role required', 'checkout')

    expect(message).toBe(
      'Ask an owner or admin to give you billing access, then try opening the secure payment page again.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('uses a safe checkout fallback when no action error is available', () => {
    expect(billingActionErrorMessage(null, 'checkout')).toBe(
      'Try opening the secure payment page again. If it still fails, ask an owner or admin to check billing.'
    )
  })

  test('uses an access-focused portal fallback when no action error is available', () => {
    expect(billingActionErrorMessage(null, 'portal')).toBe(
      'Try opening the billing management page again. If it still fails, ask an owner or admin to check billing access.'
    )
  })
})
