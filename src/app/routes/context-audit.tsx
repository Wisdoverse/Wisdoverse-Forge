import { Navigate, createRoute } from '@tanstack/react-router'
import { AuditLogView } from '@app/features/governance/AuditLogView'
import { Route as rootRoute } from './__root'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/context/audit',
  component: function ContextAuditPage() {
    const loaded = useContextFeaturesStore((s) => s.loaded)
    const enabled = useContextFeaturesStore((s) => s.governance)
    if (!loaded) {
      return (
        <div className="flex h-full items-center justify-center text-ui-body text-secondary-light dark:text-secondary-dark">
          Loading audit…
        </div>
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
