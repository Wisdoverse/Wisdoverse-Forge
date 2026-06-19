import { Navigate, createRoute } from '@tanstack/react-router'
import { AuditLogView } from '@app/features/governance/AuditLogView'
import { Route as rootRoute } from './__root'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { FeatureRouteLoadingState } from '@app/shared/ui/FeatureRouteLoadingState'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/context/audit',
  component: function ContextAuditPage() {
    const loaded = useContextFeaturesStore((s) => s.loaded)
    const enabled = useContextFeaturesStore((s) => s.governance)
    if (!loaded) {
      return (
        <FeatureRouteLoadingState
          testId="context-audit-route-loading"
          title="Checking change history"
          detail="We are confirming whether change history is available here. If this takes more than a moment, open Change history again or ask an owner or admin to check change history access."
        />
      )
    }
    if (!enabled) return <Navigate to="/tasks" />
    return (
      <div data-testid="page-context-audit" className="h-full">
        <AuditLogView />
      </div>
    )
  },
})
