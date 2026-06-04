import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { BillingPage } from '@app/features/billing/BillingPage'
import { useBillingStore } from '@app/shared/model/billing.store'
import type {
  BillingInvoice,
  BillingPlan,
  BillingSubscription,
  UsageMetric,
} from '@app/shared/api/legacy/billingApi'

const loadAllMock = vi.fn()
const createCheckoutMock = vi.fn()
const openPortalMock = vi.fn()
const originalState = useBillingStore.getState()

const plan: BillingPlan = {
  id: 'plan-team',
  name: 'Team Plan',
  description: 'Shared billing for team work.',
  features: {},
  limits: {},
  price: { monthly: 2000, yearly: 20000, currency: 'usd' },
}

const subscription: BillingSubscription = {
  id: 'sub-1',
  planId: 'plan-team',
  status: 'active',
  currentPeriodStart: '2026-05-01T00:00:00.000Z',
  currentPeriodEnd: '2026-06-01T00:00:00.000Z',
  cancelAtPeriodEnd: false,
}

const usage: UsageMetric[] = [{ metric: 'agents', current: 2, limit: 10, percentUsed: 20 }]

const invoices: BillingInvoice[] = [
  {
    id: 'invoice-1',
    number: 'INV-001',
    status: 'paid',
    amountDue: 0,
    amountPaid: 2000,
    total: 2000,
    currency: 'usd',
    createdAt: '2026-05-01T00:00:00.000Z',
  },
]

function setBillingState(overrides: Partial<typeof originalState> = {}) {
  useBillingStore.setState({
    subscription: null,
    plan: null,
    subscriptionLoading: false,
    subscriptionError: null,
    usage: [],
    usageLoading: false,
    usageError: null,
    invoices: [],
    invoicesLoading: false,
    invoicesError: null,
    billingNotConfigured: false,
    loadAll: loadAllMock,
    createCheckout: createCheckoutMock,
    openPortal: openPortalMock,
    ...overrides,
  })
}

beforeEach(() => {
  loadAllMock.mockResolvedValue(undefined)
  createCheckoutMock.mockResolvedValue(null)
  openPortalMock.mockResolvedValue(null)
  setBillingState()
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useBillingStore.setState(originalState)
})

describe('BillingPage', () => {
  test('explains the setup path when billing is not configured', async () => {
    setBillingState({ billingNotConfigured: true })

    render(<BillingPage />)

    expect(await screen.findByText('Billing setup path')).toBeDefined()
    expect(screen.getByText(/nothing can be charged/i)).toBeDefined()
    expect(screen.getByText(/do not paste payment keys/i)).toBeDefined()
    await waitFor(() => expect(loadAllMock).toHaveBeenCalled())
  })

  test('keeps upgrade disabled when no paid plan is available', async () => {
    render(<BillingPage />)

    expect(await screen.findByText('Billing checkpoint')).toBeDefined()
    expect(screen.getAllByText('No paid subscription yet').length).toBeGreaterThan(0)
    expect(screen.getByText(/no paid plan is attached yet/i)).toBeDefined()
    expect(screen.getByText(/ask an owner or admin/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /upgrade plan/i })).toBeDisabled()
    expect(screen.getByText(/invoices appear after checkout/i)).toBeDefined()
  })

  test('opens the billing portal from an active subscription', async () => {
    const openSpy = vi.spyOn(window, 'open').mockImplementation(() => null)
    openPortalMock.mockResolvedValue('https://billing.example.test/portal')
    setBillingState({ plan, subscription, usage, invoices })

    render(<BillingPage />)

    expect(await screen.findByText('Subscription is active or managed')).toBeDefined()
    expect(screen.getByText('1 usage areas visible')).toBeDefined()
    expect(screen.getByText('1 invoice records')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /manage billing/i }))

    await waitFor(() => expect(openPortalMock).toHaveBeenCalled())
    expect(openSpy).toHaveBeenCalledWith(
      'https://billing.example.test/portal',
      '_blank',
      'noopener,noreferrer'
    )
  })

  test('shows plan and usage load errors instead of silently falling back', async () => {
    setBillingState({
      subscriptionError:
        'Plan and payment could not be loaded. Ask an owner or admin for access.',
      usageError:
        'Usage could not be loaded. The app could not reach the service. Check your connection, then refresh this page.',
    })

    render(<BillingPage />)

    expect(await screen.findByText('Billing checkpoint')).toBeDefined()
    expect(screen.getAllByRole('alert')).toHaveLength(2)
    expect(
      screen.getByText(
        'Plan and payment could not be loaded. Ask an owner or admin for access.'
      )
    ).toBeDefined()
    expect(
      screen.getByText(
        'Usage could not be loaded. The app could not reach the service. Check your connection, then refresh this page.'
      )
    ).toBeDefined()
  })
})
