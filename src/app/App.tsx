import { createRouter, createRoute, redirect, RouterProvider } from '@tanstack/react-router'
import { Route as rootRoute } from './routes/__root'
import { Route as loginRoute } from './routes/login'
import { Route as startRoute } from './routes/start'
import { Route as tasksRoute } from './routes/tasks'
import { Route as inboxRoute } from './routes/inbox'
import { Route as contextRoute } from './routes/context'
import { Route as contextAuditRoute } from './routes/context-audit'
import { Route as agentsRoute } from './routes/agents'
import { Route as skillsRoute } from './routes/skills'
import { Route as settingsRoute, SectionRoute as settingsSectionRoute } from './routes/settings'
import { Route as billingRoute } from './routes/billing'
import { Route as adminRoute } from './routes/admin'
import { Route as analyticsRoute } from './routes/analytics'

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  beforeLoad: () => {
    throw redirect({ to: '/start' })
  },
})

const routeTree = rootRoute.addChildren([
  indexRoute,
  loginRoute,
  startRoute,
  tasksRoute,
  inboxRoute,
  contextRoute,
  contextAuditRoute,
  agentsRoute,
  skillsRoute,
  settingsRoute,
  settingsSectionRoute,
  billingRoute,
  adminRoute,
  analyticsRoute,
])

const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

export default function App() {
  return <RouterProvider router={router} />
}
