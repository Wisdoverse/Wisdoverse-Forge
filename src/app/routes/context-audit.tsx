import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { ContextAuditPage } from '@app/pages/context-audit'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/context/audit',
  component: ContextAuditPage,
})
