import { createRoute, redirect } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { GettingStartedView } from '@app/pages/getting-started'
import { resolveLandingPath } from './landing'

export async function skipDismissedStartRoute() {
  if ((await resolveLandingPath()) === '/tasks') {
    throw redirect({ to: '/tasks' })
  }
}

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/start',
  beforeLoad: skipDismissedStartRoute,
  component: function StartPage() {
    return <GettingStartedView />
  },
})
