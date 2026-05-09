import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type {
  BillingPlan,
  BillingSubscription,
  SubscriptionStatus,
} from '@app/shared/api/legacy/billingApi'

// ============================================================================
// Helpers
// ============================================================================

function formatCurrency(amount: number, currency: string): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: currency.toUpperCase(),
    minimumFractionDigits: 0,
  }).format(amount / 100)
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}

function statusBadge(status: SubscriptionStatus): { label: string; color: string } {
  switch (status) {
    case 'active':
      return { label: 'Active', color: 'bg-apple-blue/10 text-apple-blue' }
    case 'trialing':
      return { label: 'Trial', color: 'bg-apple-blue/10 text-apple-blue' }
    case 'past_due':
      return {
        label: 'Past Due',
        color: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
      }
    case 'canceled':
      return { label: 'Canceled', color: 'bg-apple-red/10 text-apple-red' }
    case 'unpaid':
      return { label: 'Unpaid', color: 'bg-apple-red/10 text-apple-red' }
  }
}

// ============================================================================
// PlanCard
// ============================================================================

interface PlanCardProps {
  plan: BillingPlan | null
  subscription: BillingSubscription | null
  onUpgrade: () => void
  onManage: () => void
  loading?: boolean
}

export function PlanCard({ plan, subscription, onUpgrade, onManage, loading }: PlanCardProps) {
  if (loading) {
    return (
      <div className={cn(uiStyles.cardPadded, 'animate-pulse')}>
        <div className="h-5 w-32 bg-black/10 dark:bg-white/10 rounded mb-3" />
        <div className="h-8 w-24 bg-black/10 dark:bg-white/10 rounded mb-2" />
        <div className="h-4 w-48 bg-black/10 dark:bg-white/10 rounded" />
      </div>
    )
  }

  const badge = subscription ? statusBadge(subscription.status) : null

  return (
    <div className={uiStyles.cardPadded}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            <h3 className="text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
              {plan?.name ?? 'Free Plan'}
            </h3>
            {badge && (
              <span
                className={cn(
                  'inline-flex items-center rounded-full px-2 py-0.5 text-ui-caption font-medium',
                  badge.color
                )}
              >
                {badge.label}
              </span>
            )}
          </div>

          {plan && (
            <div className="flex items-baseline gap-1 mb-2">
              <span className="text-ui-metric font-semibold text-foreground-light dark:text-foreground-dark">
                {formatCurrency(plan.price.monthly, plan.price.currency)}
              </span>
              <span className="text-ui-body text-secondary-light dark:text-secondary-dark">
                /mo
              </span>
            </div>
          )}

          {plan?.description && (
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              {plan.description}
            </p>
          )}

          {subscription && (
            <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {subscription.cancelAtPeriodEnd
                ? `Cancels on ${formatDate(subscription.currentPeriodEnd)}`
                : `Renews on ${formatDate(subscription.currentPeriodEnd)}`}
            </p>
          )}
        </div>

        <div className="flex flex-col gap-2 shrink-0">
          {subscription ? (
            <button type="button" onClick={onManage} className={uiStyles.secondaryButton}>
              Manage
            </button>
          ) : (
            <button type="button" onClick={onUpgrade} className={uiStyles.primaryButton}>
              Upgrade
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
