import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { InboxPage } from '@app/pages/inbox'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/inbox',
  component: InboxPage,
})
