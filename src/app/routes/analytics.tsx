import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { AnalyticsDashboard } from '@app/features/analytics/AnalyticsDashboard'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/analytics',
  component: function AnalyticsPage() {
    return (
      <div data-testid="page-analytics" className="h-full">
        <AnalyticsDashboard />
      </div>
    )
  },
})
