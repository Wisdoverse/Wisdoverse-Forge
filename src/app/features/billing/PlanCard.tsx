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

function statusBadge(status: SubscriptionStatus): {
  label: string
  description: string
  color: string
} {
  switch (status) {
    case 'active':
      return {
        label: 'Plan active',
        description: 'Your team can keep using the capacity included with this plan.',
        color: 'bg-apple-blue/10 text-apple-blue',
      }
    case 'trialing':
      return {
        label: 'Trial active',
        description: 'Review usage before the trial ends so there are no surprises.',
        color: 'bg-apple-blue/10 text-apple-blue',
      }
    case 'past_due':
      return {
        label: 'Payment due',
        description: 'Update your payment method to keep the plan active.',
        color: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
      }
    case 'canceled':
      return {
        label: 'Plan canceled',
        description: 'Upgrade again when the team needs paid capacity.',
        color: 'bg-apple-red/10 text-apple-red',
      }
    case 'unpaid':
      return {
        label: 'Payment needed',
        description: 'Resolve the outstanding balance to restore access.',
        color: 'bg-apple-red/10 text-apple-red',
      }
  }
}

function nextStep(plan: BillingPlan | null, subscription: BillingSubscription | null): string {
  if (!subscription) {
    return plan
      ? 'Upgrade only when your team needs this paid capacity.'
      : 'Start here. Upgrade when your team needs more agents, history, or AI usage.'
  }

  if (subscription.cancelAtPeriodEnd) {
    return `The plan will stop on ${formatDate(subscription.currentPeriodEnd)}. Manage billing to resume it before that date.`
  }

  switch (subscription.status) {
    case 'active':
      return 'No action needed now. Manage billing for receipts, payment details, or cancellation.'
    case 'trialing':
      return `The trial runs until ${formatDate(subscription.currentPeriodEnd)}. Check usage before it ends.`
    case 'past_due':
    case 'unpaid':
      return 'Update your payment method now. Unpaid plans lose access when the retry window closes.'
    case 'canceled':
      return 'Upgrade to start a new plan.'
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
  actionPending?: 'checkout' | 'portal' | null
  actionError?: string | null
}

export function PlanCard({
  plan,
  subscription,
  onUpgrade,
  onManage,
  loading,
  actionPending: _actionPending,
  actionError,
}: PlanCardProps) {
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
  const canUpgrade = Boolean(plan)
  const priceLabel = plan ? formatCurrency(plan.price.monthly, plan.price.currency) : '$0'

  return (
    <div className={cn(uiStyles.cardPadded, 'flex flex-col gap-4')}>
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex-1 min-w-0">
          <div className="mb-1 flex flex-wrap items-center gap-2">
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

          <div className="mb-2 flex items-baseline gap-1">
            <span className="text-ui-metric font-semibold text-foreground-light dark:text-foreground-dark">
              {priceLabel}
            </span>
            <span className="text-ui-body text-secondary-light dark:text-secondary-dark">/mo</span>
          </div>

          <p className="max-w-2xl text-ui-body text-secondary-light dark:text-secondary-dark">
            {plan?.description ??
              'No paid plan is active yet. You can keep working until the team needs more capacity.'}
          </p>

          {!plan && !subscription && (
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No paid plan is attached yet. An administrator must publish a billing plan before
              checkout is available.
            </p>
          )}

          {subscription && badge && (
            <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {badge.description}
            </p>
          )}

          <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {subscription
              ? 'Use the billing portal to update payment methods, invoices, or cancellation.'
              : canUpgrade
                ? 'Upgrade opens checkout in this browser. Review the plan before continuing.'
                : 'Ask an owner or administrator to make a plan available.'}
          </p>
        </div>

        <div className="flex shrink-0 flex-col gap-2 sm:items-end">
          {subscription ? (
            <button type="button" onClick={onManage} className={uiStyles.secondaryButton}>
              Manage billing
            </button>
          ) : (
            <button
              type="button"
              onClick={onUpgrade}
              disabled={!canUpgrade}
              className={uiStyles.primaryButton}
            >
              Upgrade plan
            </button>
          )}
        </div>
      </div>

      <div className="rounded-card border border-black/[0.08] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.03]">
        <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
          What to do next
        </p>
        <p className="mt-0.5 text-ui-body text-secondary-light dark:text-secondary-dark">
          {nextStep(plan, subscription)}
        </p>
        {subscription && !subscription.cancelAtPeriodEnd && (
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Renews on {formatDate(subscription.currentPeriodEnd)}
          </p>
        )}
      </div>

      {actionError && (
        <p role="alert" className="text-ui-body text-apple-red">
          {actionError}
        </p>
      )}
    </div>
  )
}
