import { Navigate, createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { ApprovalQueueView } from '@app/features/context/ApprovalQueueView'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/context',
  component: function ContextApprovalPage() {
    const loaded = useContextFeaturesStore((s) => s.loaded)
    const enabled = useContextFeaturesStore((s) => s.governance)
    if (!loaded) {
      return (
        <div className="flex h-full items-center justify-center text-ui-body text-secondary-light dark:text-secondary-dark">
          Loading context…
        </div>
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
