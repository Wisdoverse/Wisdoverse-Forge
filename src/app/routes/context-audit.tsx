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
          title="Checking audit access"
          detail="We are confirming whether governance audit is enabled for this workspace. If this takes more than a moment, refresh the page or ask an owner or admin to check workspace setup."
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
