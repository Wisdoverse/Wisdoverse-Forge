import { Navigate } from '@tanstack/react-router'
import { AuditLogView } from '@app/features/governance'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { FeatureRouteLoadingState } from '@app/shared/ui/FeatureRouteLoadingState'

export function ContextAuditPage() {
  const loaded = useContextFeaturesStore((state) => state.loaded)
  const enabled = useContextFeaturesStore((state) => state.governance)
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
}
