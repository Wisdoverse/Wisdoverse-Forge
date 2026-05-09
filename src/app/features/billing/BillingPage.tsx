import { useEffect } from 'react'
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
        Billing not configured
      </h2>
      <p className="max-w-sm text-ui-body text-secondary-light dark:text-secondary-dark">
        Stripe integration is not enabled on this deployment. Contact your administrator to set up
        billing.
      </p>
    </div>
  )
}

// ============================================================================
// BillingPage
// ============================================================================

export function BillingPage() {
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
    if (!plan) return
    const url = await createCheckout({
      planId: plan.id,
      billingCycle: 'monthly',
      successUrl: window.location.href,
      cancelUrl: window.location.href,
    })
    if (url) window.location.href = url
  }

  const handleManage = async () => {
    const url = await openPortal()
    if (url) window.open(url, '_blank', 'noopener,noreferrer')
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
      {/* Current Plan */}
      <section>
        <h3 className={uiStyles.groupLabel}>Current Plan</h3>
        <PlanCard
          plan={plan}
          subscription={subscription}
          loading={subscriptionLoading}
          onUpgrade={() => void handleUpgrade()}
          onManage={() => void handleManage()}
        />
      </section>

      {/* Usage */}
      {(usageLoading || usage.length > 0) && (
        <section>
          <h3 className={uiStyles.groupLabel}>Usage</h3>
          <UsageMeter metrics={usage} loading={usageLoading} />
        </section>
      )}

      {/* Invoices */}
      <section>
        <InvoiceList invoices={invoices} loading={invoicesLoading} error={invoicesError} />
      </section>
    </div>
  )
}
