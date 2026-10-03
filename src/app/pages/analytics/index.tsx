import { AnalyticsDashboard } from '@app/features/analytics'
import { MaintenanceOutcomesDashboard } from '@app/features/maintenance-delivery'

export function AnalyticsPage() {
  return (
    <div data-testid="page-analytics" className="h-full overflow-y-auto">
      <MaintenanceOutcomesDashboard />
      <AnalyticsDashboard />
    </div>
  )
}
