import { beforeEach, describe, expect, test, vi } from 'vitest'
import { useBillingStore } from '@app/features/billing/model/billing.store'

const billingApiMock = vi.hoisted(() => ({
  getSubscription: vi.fn(),
  getUsage: vi.fn(),
  getInvoices: vi.fn(),
  createCheckout: vi.fn(),
  createPortalAgent: vi.fn(),
}))

vi.mock('@app/shared/api/legacy', () => ({
  getBillingApi: () => billingApiMock,
}))

const initialState = useBillingStore.getState()

describe('useBillingStore beginner errors', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useBillingStore.setState(initialState, true)
  })

  test('keeps billing service failures out of the not-configured state', async () => {
    billingApiMock.getSubscription.mockRejectedValueOnce({
      statusCode: 503,
      message: 'database unavailable while loading billing',
    })

    await useBillingStore.getState().loadSubscription()

    expect(useBillingStore.getState().billingNotConfigured).toBe(false)
    expect(useBillingStore.getState().subscriptionError).toBe(
      'Choose Check billing again to load plan and payment. If it still fails, ask an owner or admin to check billing.'
    )
    expect(useBillingStore.getState().subscriptionError).not.toContain('database unavailable')
  })
})
