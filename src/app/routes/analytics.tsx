import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { AnalyticsPage } from '@app/pages/analytics'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/analytics',
  component: AnalyticsPage,
})
