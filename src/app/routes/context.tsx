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
          title="Checking saved notes review"
          detail="We are checking whether saved notes review is available here. If this takes more than a moment, refresh the page or ask an owner or admin to check saved items setup."
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
