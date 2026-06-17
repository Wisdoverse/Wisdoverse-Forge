import { useEffect, useState } from 'react'
import { CreditCard } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useBillingStore } from '@app/shared/model/billing.store'
import { PlanCard } from './PlanCard'
import { UsageMeter } from './UsageMeter'
import { InvoiceList } from './InvoiceList'

const BILLING_SETUP_STEPS = [
  'Ask an owner or admin to turn on billing for this workspace.',
  'Do not paste secret payment settings here. Ask an owner or admin to connect billing in settings.',
  'Refresh this page after billing is turned on.',
]

// ============================================================================
// Not configured state
// ============================================================================

function BillingNotConfigured() {
  return (
    <div
      className={cn(
        uiStyles.cardPadded,
        'min-h-72 px-6 py-12',
        'flex flex-col items-center justify-center gap-3 text-center'
      )}
    >
      <div className="flex h-12 w-12 items-center justify-center rounded-full bg-apple-blue/10 text-apple-blue">
        <CreditCard size={20} strokeWidth={2} aria-hidden="true" />
      </div>
      <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
        Billing is not ready yet
      </h2>
      <p className="max-w-sm text-ui-body text-secondary-light dark:text-secondary-dark">
        Billing is not enabled for this workspace yet. Ask an owner or admin to connect billing
        before changing plans or payment methods.
      </p>
      <div className="mt-2 max-w-sm text-left">
        <p className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
          Billing setup path
        </p>
        <ol className="mt-2 list-decimal space-y-1 pl-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {BILLING_SETUP_STEPS.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      </div>
    </div>
  )
}

interface BillingCheckpointProps {
  hasSubscription: boolean
  usageCount: number
  invoicesCount: number
}

function formatCapacityCheckCount(count: number): string {
  return `${count} capacity ${count === 1 ? 'check' : 'checks'} shown`
}

function BillingCheckpoint({ hasSubscription, usageCount, invoicesCount }: BillingCheckpointProps) {
  const checkpoints = [
    {
      label: 'Plan',
      value: hasSubscription
        ? 'Paid plan is active'
        : 'Free plan is active. Ask an owner or admin to choose a paid plan when the team is ready.',
    },
    {
      label: 'Capacity',
      value:
        usageCount > 0
          ? formatCapacityCheckCount(usageCount)
          : 'Capacity details appear after agents run billable work',
    },
    {
      label: 'Invoices',
      value:
        invoicesCount > 0 ? `${invoicesCount} invoices shown` : 'Invoices appear after a charge',
    },
  ]

  return (
    <section
      aria-label="Billing checkpoint"
      className="rounded-lg border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
      <div className="flex flex-col gap-1">
        <h2 className={uiStyles.sectionTitle}>Billing checkpoint</h2>
        <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
          Review the plan, capacity, and invoices before you change a plan or payment method.
        </p>
      </div>
      <div className="mt-3 grid gap-2 sm:grid-cols-3">
        {checkpoints.map((checkpoint) => (
          <div
            key={checkpoint.label}
            className="rounded-md bg-black/[0.025] px-3 py-2 dark:bg-white/[0.04]"
          >
            <span className="block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              {checkpoint.label}
            </span>
            <span className="mt-0.5 block text-ui-caption text-foreground-light dark:text-foreground-dark">
              {checkpoint.value}
            </span>
          </div>
        ))}
      </div>
    </section>
  )
}

// ============================================================================
// BillingPage
// ============================================================================

export function BillingPage() {
  const [billingAction, setBillingAction] = useState<'checkout' | 'portal' | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const {
    subscription,
    plan,
    subscriptionLoading,
    subscriptionError,
    usage,
    usageLoading,
    usageError,
    invoices,
    invoicesLoading,
    invoicesError,
    billingNotConfigured,
    loadAll,
    createCheckout,
    openPortal,
  } = useBillingStore()

  useEffect(() => {
    void loadAll()
  }, [loadAll])

  const handleUpgrade = async () => {
    if (!plan) {
      setActionError(
        'Ask an owner or admin to make a paid plan available before opening the secure payment page.'
      )
      return
    }

    setActionError(null)
    setBillingAction('checkout')
    try {
      const url = await createCheckout({
        planId: plan.id,
        billingCycle: 'monthly',
        successUrl: window.location.href,
        cancelUrl: window.location.href,
      })
      if (url) {
        window.location.href = url
        return
      }
      setActionError(
        'Try opening the secure payment page again. If it still does not open, ask an owner or admin to check billing.'
      )
    } finally {
      setBillingAction(null)
    }
  }

  const handleManage = async () => {
    setActionError(null)
    setBillingAction('portal')
    try {
      const url = await openPortal()
      if (url) {
        window.open(url, '_blank', 'noopener,noreferrer')
        return
      }
      setActionError(
        'Try opening the billing management page again. If it still does not open, ask an owner or admin to check access.'
      )
    } finally {
      setBillingAction(null)
    }
  }

  if (billingNotConfigured) {
    return (
      <div className="p-4 sm:p-6">
        <BillingNotConfigured />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6 p-4 sm:p-6">
      <BillingCheckpoint
        hasSubscription={Boolean(subscription)}
        usageCount={usage.length}
        invoicesCount={invoices.length}
      />

      {/* Current Plan */}
      <section>
        <h3 className={uiStyles.groupLabel}>Plan and payment</h3>
        {subscriptionError && (
          <div role="alert" aria-live="polite" className={cn(uiStyles.error, 'mb-3')}>
            {subscriptionError}
          </div>
        )}
        <PlanCard
          plan={plan}
          subscription={subscription}
          loading={subscriptionLoading}
          actionPending={billingAction}
          actionError={actionError}
          onUpgrade={() => void handleUpgrade()}
          onManage={() => void handleManage()}
        />
      </section>

      {(usageLoading || usage.length > 0 || usageError) && (
        <section>
          <h3 className={uiStyles.groupLabel}>Capacity this period</h3>
          {usageError ? (
            <div role="alert" aria-live="polite" className={uiStyles.error}>
              {usageError}
            </div>
          ) : (
            <UsageMeter metrics={usage} loading={usageLoading} />
          )}
        </section>
      )}

      <section>
        <InvoiceList invoices={invoices} loading={invoicesLoading} error={invoicesError} />
      </section>
    </div>
  )
}
