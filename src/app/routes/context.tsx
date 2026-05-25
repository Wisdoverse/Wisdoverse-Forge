import { Navigate, createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { ApprovalQueueView } from '@app/features/context/ApprovalQueueView'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { FeatureRouteLoadingState } from '@app/shared/ui/FeatureRouteLoadingState'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/context',
  component: function ContextApprovalPage() {
    const loaded = useContextFeaturesStore((s) => s.loaded)
    const enabled = useContextFeaturesStore((s) => s.governance)
    if (!loaded) {
      return (
        <FeatureRouteLoadingState
          testId="context-route-loading"
          title="Checking context review"
          detail="We are confirming whether context review is enabled for this workspace. If this takes more than a moment, refresh the page or ask an administrator to check setup."
        />
      )
    }
    if (!enabled) return <Navigate to="/tasks" />
    return (
      <div data-testid="page-context" className="h-full">
        <ApprovalQueueView />
      </div>
    )
  },
})
