import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { BillingPage } from '@app/features/billing/BillingPage'
import { useBillingStore } from '@app/features/billing/model/billing.store'
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
  test('explains the setup steps when billing is not configured', async () => {
    setBillingState({ billingNotConfigured: true })

    render(<BillingPage />)

    expect(await screen.findByText('What to do next')).toBeDefined()
    expect(
      screen.getByText(/connect billing before changing plans or payment methods/i)
    ).toBeDefined()
    expect(screen.getByText(/turn on billing for this team/i)).toBeDefined()
    expect(screen.getByText(/payment account passwords or keys/i)).toBeDefined()
    expect(screen.getByText(/open Billing from the sidebar/i)).toBeDefined()
    expect(screen.queryByText(/Refresh this page after billing is turned on/i)).toBeNull()
    expect(screen.queryByText(/this workspace/i)).toBeNull()
    expect(screen.queryByText(/secret payment settings/i)).toBeNull()
    expect(screen.queryByText(/deployment/i)).toBeNull()
    await waitFor(() => expect(loadAllMock).toHaveBeenCalled())
  })

  test('keeps upgrade disabled when no paid plan is available', async () => {
    render(<BillingPage />)

    expect(await screen.findByText('Billing checkpoint')).toBeDefined()
    expect(screen.getByText(/Free plan is active/i)).toBeDefined()
    expect(screen.queryByText('No paid plan yet')).toBeNull()
    expect(screen.getByText('Capacity details appear after agents run billable work')).toBeDefined()
    expect(screen.getByText(/make a paid plan available/i)).toBeDefined()
    expect(screen.queryByText(/no paid plan is attached yet/i)).toBeNull()
    expect(screen.getByText(/secure payment page can open/i)).toBeDefined()
    expect(screen.getByText('Ask an owner or admin to make a plan available.')).toBeDefined()
    expect(screen.getByRole('button', { name: /upgrade plan/i })).toBeDisabled()
    expect(screen.getByText('Invoices appear after a charge')).toBeDefined()
    expect(screen.getByText('Start or change a plan to create the first invoice')).toBeDefined()
    expect(screen.queryByText('No usage reported yet')).toBeNull()
    expect(screen.queryByText(/checkout/i)).toBeNull()
    expect(screen.queryByText(/billing portal/i)).toBeNull()
  })

  test('opens the billing management page from an active subscription', async () => {
    const openSpy = vi.spyOn(window, 'open').mockImplementation(() => null)
    openPortalMock.mockResolvedValue('https://billing.example.test/portal')
    setBillingState({ plan, subscription, usage, invoices })

    render(<BillingPage />)

    expect(await screen.findByText('Paid plan is active')).toBeDefined()
    expect(screen.getByText('1 capacity check shown')).toBeDefined()
    expect(screen.getByText('1 invoices shown')).toBeDefined()
    expect(screen.queryByText(/usage areas shown/i)).toBeNull()

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
        'Ask an owner or admin to give you billing access, then choose Check billing again to load plan and payment.',
      usageError:
        'Check your connection, then choose Check billing again to load usage. Forge could not connect while loading billing.',
    })

    render(<BillingPage />)

    expect(await screen.findByText('Billing checkpoint')).toBeDefined()
    expect(screen.getAllByRole('alert')).toHaveLength(2)
    expect(
      screen.getByText(
        'Ask an owner or admin to give you billing access, then choose Check billing again to load plan and payment.'
      )
    ).toBeDefined()
    expect(
      screen.getByText(
        'Check your connection, then choose Check billing again to load usage. Forge could not connect while loading billing.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/Refresh Billing/i)).toBeNull()
    const retryButtons = screen.getAllByRole('button', { name: 'Check billing again' })
    expect(retryButtons).toHaveLength(2)
    fireEvent.click(retryButtons[0]!)
    await waitFor(() => expect(loadAllMock).toHaveBeenCalledTimes(2))
  })

  test('starts checkout recovery with the retry action', async () => {
    setBillingState({ plan })

    render(<BillingPage />)

    fireEvent.click(await screen.findByRole('button', { name: /upgrade plan/i }))

    await waitFor(() => expect(createCheckoutMock).toHaveBeenCalled())
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Try opening the secure payment page again. If it still fails, ask an owner or admin to check billing.'
    )
  })

  test('starts billing management recovery with the retry action', async () => {
    setBillingState({ plan, subscription })

    render(<BillingPage />)

    fireEvent.click(await screen.findByRole('button', { name: /manage billing/i }))

    await waitFor(() => expect(openPortalMock).toHaveBeenCalled())
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Try opening the billing management page again. If it still fails, ask an owner or admin to check billing access.'
    )
  })
})
