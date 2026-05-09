import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { BillingPage } from '@app/features/billing/BillingPage'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/billing',
  component: function BillingRoute() {
    return (
      <div data-testid="page-billing" className="h-full overflow-y-auto px-8 py-8 max-w-3xl">
        <BillingPage />
      </div>
    )
  },
})
