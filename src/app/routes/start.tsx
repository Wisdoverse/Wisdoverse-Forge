import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { GettingStartedView } from '@app/pages/getting-started'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/start',
  component: function StartPage() {
    return <GettingStartedView />
  },
})
