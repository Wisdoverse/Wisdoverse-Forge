import { useEffect, useState } from 'react'
import { CreditCard } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useBillingStore } from '@app/shared/model/billing.store'
import { PlanCard } from './PlanCard'
import { UsageMeter } from './UsageMeter'
import { InvoiceList } from './InvoiceList'

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
        Billing is not turned on
      </h2>
      <p className="max-w-sm text-ui-body text-secondary-light dark:text-secondary-dark">
        Ask an administrator to enable billing before you change plans, review usage limits, or view
        invoices.
      </p>
    </div>
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
    usage,
    usageLoading,
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
      setActionError('A paid plan must be available before checkout can open.')
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
      setActionError('Checkout did not open. Try again or ask an administrator to check billing.')
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
        'Billing management did not open. Try again or ask an administrator to check access.'
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
      <header>
        <h1 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          Billing
        </h1>
        <p className="mt-1 max-w-2xl text-ui-body text-secondary-light dark:text-secondary-dark">
          Review the current plan, watch the limits that can block work, and open invoices when
          finance needs a record.
        </p>
      </header>

      <section>
        <h2 className={uiStyles.groupLabel}>Current Plan</h2>
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

      {(usageLoading || usage.length > 0) && (
        <section>
          <h2 className={uiStyles.groupLabel}>Usage</h2>
          <UsageMeter metrics={usage} loading={usageLoading} />
        </section>
      )}

      <section>
        <InvoiceList invoices={invoices} loading={invoicesLoading} error={invoicesError} />
      </section>
    </div>
  )
}
