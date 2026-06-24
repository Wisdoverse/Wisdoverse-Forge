import { Navigate } from '@tanstack/react-router'
import { ApprovalQueueView } from '@app/features/context/ApprovalQueueView'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { FeatureRouteLoadingState } from '@app/shared/ui/FeatureRouteLoadingState'

export function ContextApprovalPage() {
  const loaded = useContextFeaturesStore((state) => state.loaded)
  const enabled = useContextFeaturesStore((state) => state.governance)
  if (!loaded) {
    return (
      <FeatureRouteLoadingState
        testId="context-route-loading"
        title="Checking saved items"
        detail="We are checking whether saved items are available here. If this takes more than a moment, open Saved items again or ask an owner or admin to check Saved items access."
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
