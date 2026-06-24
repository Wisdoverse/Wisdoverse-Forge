import { BillingPage as BillingFeaturePage } from '@app/features/billing/BillingPage'

export function BillingPage() {
  return (
    <div data-testid="page-billing" className="h-full max-w-3xl overflow-y-auto px-8 py-8">
      <BillingFeaturePage />
    </div>
  )
}
