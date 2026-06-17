import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { BillingPage } from '@app/features/billing/BillingPage'
import { InvoiceList } from '@app/features/billing/InvoiceList'
import { PlanCard } from '@app/features/billing/PlanCard'
import { UsageMeter } from '@app/features/billing/UsageMeter'
import { useBillingStore } from '@app/shared/model/billing.store'
import type {
  BillingInvoice,
  BillingPlan,
  BillingSubscription,
  UsageMetric,
} from '@app/shared/api/legacy/billingApi'

const originalBillingState = useBillingStore.getState()

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useBillingStore.setState(originalBillingState, true)
})

const teamPlan: BillingPlan = {
  id: 'team',
  name: 'Team Plan',
  description: 'More capacity for production teams.',
  features: {},
  limits: {},
  price: { monthly: 2900, yearly: 29000, currency: 'usd' },
}

const activeSubscription: BillingSubscription = {
  id: 'sub_1',
  planId: 'team',
  status: 'active',
  currentPeriodStart: '2026-05-01T00:00:00.000Z',
  currentPeriodEnd: '2026-06-01T00:00:00.000Z',
  cancelAtPeriodEnd: false,
}

describe('Billing beginner guidance', () => {
  test('explains the billing page and the not-enabled state without setup jargon', () => {
    useBillingStore.setState({
      billingNotConfigured: true,
      loadAll: vi.fn().mockResolvedValue(undefined),
    } as never)

    render(<BillingPage />)

    expect(screen.getByText('Billing is not ready yet')).toBeInTheDocument()
    expect(screen.getByText(/Billing is not turned on for this team yet/i)).toBeInTheDocument()
    expect(screen.getByText('What to do next')).toBeInTheDocument()
    expect(screen.getByText(/turn on billing for this team/i)).toBeInTheDocument()
    expect(screen.getByText(/payment account passwords or keys/i)).toBeInTheDocument()
    expect(screen.queryByText('Billing setup path')).not.toBeInTheDocument()
    expect(screen.queryByText(/this workspace/i)).not.toBeInTheDocument()
    expect(screen.queryByText(/secret payment settings/i)).not.toBeInTheDocument()
    expect(screen.queryByText(/deployment/i)).not.toBeInTheDocument()
    expect(screen.queryByText(/integration/i)).not.toBeInTheDocument()
  })

  test('shows clear plan status and a next step for paid subscriptions', () => {
    render(
      <PlanCard
        plan={teamPlan}
        subscription={{ ...activeSubscription, status: 'past_due' }}
        onUpgrade={vi.fn()}
        onManage={vi.fn()}
      />
    )

    expect(screen.getByText('Team Plan')).toBeInTheDocument()
    expect(screen.getByText('$29')).toBeInTheDocument()
    expect(screen.getByText('Payment due')).toBeInTheDocument()
    expect(screen.getByText(/Update your payment method to keep the plan active/i)).toBeDefined()
    expect(screen.getByText(/billing management page/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /manage billing/i })).toHaveTextContent(
      'Manage billing'
    )
  })

  test('makes the free plan state understandable before upgrade', () => {
    render(<PlanCard plan={null} subscription={null} onUpgrade={vi.fn()} onManage={vi.fn()} />)

    expect(screen.getByText('Free Plan')).toBeInTheDocument()
    expect(screen.getByText('$0')).toBeInTheDocument()
    expect(screen.getByText(/No paid plan is active yet/i)).toBeInTheDocument()
    expect(screen.getByText(/Upgrade when your team needs more agents/i)).toBeInTheDocument()
    expect(screen.getByText(/AI message use/i)).toBeInTheDocument()
    expect(screen.queryByText(new RegExp('AI text\\s+usage', 'i'))).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /upgrade plan/i })).toHaveTextContent('Upgrade plan')
  })

  test('shows when the secure payment page is opening', () => {
    render(
      <PlanCard
        plan={teamPlan}
        subscription={null}
        actionPending="checkout"
        onUpgrade={vi.fn()}
        onManage={vi.fn()}
      />
    )

    const button = screen.getByRole('button', { name: /opening payment page/i })
    expect(button).toBeDisabled()
    expect(button).toHaveTextContent('Opening payment page...')
  })

  test('shows when the billing management page is opening', () => {
    render(
      <PlanCard
        plan={teamPlan}
        subscription={activeSubscription}
        actionPending="portal"
        onUpgrade={vi.fn()}
        onManage={vi.fn()}
      />
    )

    const button = screen.getByRole('button', { name: /opening billing page/i })
    expect(button).toBeDisabled()
    expect(button).toHaveTextContent('Opening billing page...')
  })

  test('translates usage metrics into plain-language capacity signals', () => {
    const metrics: UsageMetric[] = [
      { metric: 'agents', current: 9, limit: 10, percentUsed: 90 },
      { metric: 'events', current: 40, limit: 100, percentUsed: 40 },
      { metric: 'tokens', current: 1200, limit: 0, percentUsed: 0 },
    ]

    render(<UsageMeter metrics={metrics} />)

    expect(screen.getByText('Agents')).toBeInTheDocument()
    expect(screen.getByText('Almost full')).toBeInTheDocument()
    expect(screen.getByText(/Archive unused agents or upgrade/i)).toBeInTheDocument()
    expect(screen.getByText('Work update history')).toBeInTheDocument()
    expect(
      screen.getByText(/Work updates, change history, and timeline messages/i)
    ).toBeInTheDocument()
    expect(screen.queryByText('Activity events')).not.toBeInTheDocument()
    expect(screen.queryByText(/audit records/i)).not.toBeInTheDocument()
    expect(screen.getByText('AI message use')).toBeInTheDocument()
    expect(screen.getByText(/Messages and replies processed/i)).toBeInTheDocument()
    expect(screen.getByText('No limit set')).toBeInTheDocument()
    expect(screen.getByText('1.2K used')).toBeInTheDocument()
    expect(screen.queryByText(/tokens/i)).not.toBeInTheDocument()
  })

  test('gives invoices beginner-safe status descriptions and record links', () => {
    const invoices: BillingInvoice[] = [
      {
        id: 'inv_paid_1234567890',
        number: 'INV-100',
        status: 'paid',
        amountDue: 0,
        amountPaid: 2900,
        total: 2900,
        currency: 'usd',
        createdAt: '2026-05-10T00:00:00.000Z',
        pdfUrl: 'https://billing.example.test/inv-100.pdf',
      },
      {
        id: 'inv_open_1234567890',
        status: 'open',
        amountDue: 2900,
        amountPaid: 0,
        total: 2900,
        currency: 'usd',
        createdAt: '2026-05-12T00:00:00.000Z',
        hostedInvoiceUrl: 'https://billing.example.test/inv-open',
      },
      {
        id: 'inv_draft_1234567890',
        status: 'draft',
        amountDue: 0,
        amountPaid: 0,
        total: 2900,
        currency: 'usd',
        createdAt: '2026-05-14T00:00:00.000Z',
      },
    ]

    render(<InvoiceList invoices={invoices} />)

    expect(
      screen.getByText(/Invoices appear after you start or change a plan/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/billing portal/i)).not.toBeInTheDocument()
    expect(screen.getByText('Paid')).toBeInTheDocument()
    expect(screen.getByText('No action needed.')).toBeInTheDocument()
    expect(screen.getByText('Payment due')).toBeInTheDocument()
    expect(screen.getByText('Pay this invoice to keep your plan active.')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /download/i })).toHaveAttribute(
      'href',
      'https://billing.example.test/inv-100.pdf'
    )
    expect(screen.getByRole('link', { name: /open/i })).toHaveAttribute(
      'href',
      'https://billing.example.test/inv-open'
    )
    expect(screen.getByText('Receipt appears after payment finishes')).toBeInTheDocument()
    expect(screen.queryByText('No link')).not.toBeInTheDocument()
  })

  test('keeps invoice empty and error states actionable', () => {
    const { rerender } = render(<InvoiceList invoices={[]} />)

    expect(screen.getByText('Invoices appear after your first charge')).toBeInTheDocument()
    expect(screen.getByText(/Receipts and payment links/i)).toBeInTheDocument()
    expect(screen.getByText(/start or change a plan/i)).toBeInTheDocument()
    expect(screen.queryByText('No invoices have been created yet')).not.toBeInTheDocument()

    rerender(
      <InvoiceList
        invoices={[]}
        error="Refresh Billing to load invoices. Ask an owner or admin for access."
      />
    )

    const alert = screen.getByRole('alert')
    expect(
      within(alert).getByText('Refresh Billing to load invoices. Ask an owner or admin for access.')
    ).toBeInTheDocument()
    expect(within(alert).getByText(/ask an owner or admin to check billing access/i)).toBeDefined()
  })
})
