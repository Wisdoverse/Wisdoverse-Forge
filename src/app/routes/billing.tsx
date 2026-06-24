import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { BillingPage } from '@app/pages/billing'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/billing',
  component: BillingPage,
})
