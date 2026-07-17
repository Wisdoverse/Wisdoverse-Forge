import { Navigate } from '@tanstack/react-router'
import { ApprovalQueueView } from '@app/features/context'
import { useContextFeaturesStore } from '@app/entities/context/model/context-features.store'
import { FeatureRouteLoadingState } from '@app/shared/ui/FeatureRouteLoadingState'

export function ContextApprovalPage() {
  const loaded = useContextFeaturesStore((state) => state.loaded)
  const enabled = useContextFeaturesStore((state) => state.governance)
  if (!loaded) {
    return (
      <FeatureRouteLoadingState
        testId="context-route-loading"
        title="Checking context items"
        detail="We are checking whether context items are available here. If this takes more than a moment, open Context again or ask an owner or admin to check Context access."
      />
    )
  }
  if (!enabled) return <Navigate to="/tasks" />
  return (
    <div data-testid="page-context" className="h-full">
      <ApprovalQueueView />
    </div>
  )
}
