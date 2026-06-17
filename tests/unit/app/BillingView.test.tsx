import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { BillingPage } from '@app/features/billing/BillingPage'
import { InvoiceList } from '@app/features/billing/InvoiceList'
import { PlanCard } from '@app/features/billing/PlanCard'
import { useBillingStore } from '@app/shared/model/billing.store'
import type {
  BillingInvoice,
  BillingPlan,
  BillingSubscription,
} from '@app/shared/api/legacy/billingApi'

const originalBillingState = useBillingStore.getState()

afterEach(() => {
  cleanup()
  useBillingStore.setState(originalBillingState, true)
  vi.restoreAllMocks()
})

describe('Billing views', () => {
  test('explains unavailable billing setup in plain language', async () => {
    const loadAll = vi.fn()
    useBillingStore.setState({
      ...originalBillingState,
      billingNotConfigured: true,
      loadAll,
    })

    render(<BillingPage />)

    await waitFor(() => expect(loadAll).toHaveBeenCalledOnce())
    expect(screen.getByText('Billing is not ready yet')).toBeDefined()
    expect(
      screen.getByText(/connect billing before changing plans or payment methods/i)
    ).toBeDefined()
    expect(screen.getByText('Billing setup path')).toBeDefined()
  })

  test('guides users when there are no invoices yet', () => {
    render(<InvoiceList invoices={[]} />)

    expect(screen.getByText('Invoices and receipts')).toBeDefined()
    expect(screen.getByText('Invoices appear after your first charge')).toBeDefined()
    expect(screen.getByText(/Receipts and payment links/i)).toBeDefined()
    expect(screen.getByText(/start or change a plan/i)).toBeDefined()
    expect(screen.queryByText('No invoices have been created yet')).toBeNull()
  })

  test('uses payment-focused invoice labels', () => {
    const invoice: BillingInvoice = {
      id: 'in_123456789012',
      number: 'INV-100',
      status: 'open',
      amountDue: 2500,
      amountPaid: 0,
      total: 2500,
      currency: 'usd',
      pdfUrl: 'https://example.com/invoice.pdf',
      createdAt: '2026-05-01T12:00:00.000Z',
    }

    render(<InvoiceList invoices={[invoice]} />)

    expect(screen.getByText('Payment due')).toBeDefined()
    expect(screen.getByText('Receipt')).toBeDefined()
    expect(screen.getByRole('link', { name: 'Download' })).toHaveAttribute('href', invoice.pdfUrl)
  })

  test('uses action labels that describe the billing outcome', () => {
    const plan: BillingPlan = {
      id: 'pro',
      name: 'Pro',
      description: 'For teams running managed agents',
      features: {},
      limits: {},
      price: { monthly: 4900, yearly: 49000, currency: 'usd' },
    }
    const subscription: BillingSubscription = {
      id: 'sub-1',
      planId: 'pro',
      status: 'past_due',
      currentPeriodStart: '2026-05-01T00:00:00.000Z',
      currentPeriodEnd: '2026-06-01T00:00:00.000Z',
      cancelAtPeriodEnd: false,
    }

    render(
      <PlanCard plan={plan} subscription={subscription} onUpgrade={vi.fn()} onManage={vi.fn()} />
    )

    expect(screen.getByText('Payment due')).toBeDefined()
    expect(screen.getByRole('button', { name: 'Manage billing' })).toBeDefined()
  })
})
