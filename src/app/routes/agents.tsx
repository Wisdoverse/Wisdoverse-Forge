import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { AgentsPage } from '@app/pages/agents'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/agents',
  component: AgentsPage,
})
