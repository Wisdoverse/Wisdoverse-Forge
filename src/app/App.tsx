import { createRouter, createRoute, redirect, RouterProvider } from '@tanstack/react-router'
import { useSettingsStore } from '@app/shared/model/settings.store'
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

// Post-login landing: first-time users get the Getting Started checklist,
// users who skipped or completed it land on the task board. The root route's
// beforeLoad already redirected unauthenticated visitors to /login, so this
// only runs with a token present. Any preferences failure falls back to the
// previous behavior (/start) — landing must never block on this request.
//
// On a fresh page load this beforeLoad fires before AuthProvider's mount
// effect has initialised the legacy API client singletons, so the store's
// loadPreferences() (which uses that client) cannot be called yet. Use the
// store when it is already warm (client-side navigation to '/'), otherwise
// read the preference with one direct authorized request.
async function resolveLandingPath(): Promise<'/start' | '/tasks'> {
  try {
    const cached = useSettingsStore.getState()
    if (cached.preferencesLoaded) {
      return cached.preferences?.gettingStartedDismissed === true ? '/tasks' : '/start'
    }

    const token = localStorage.getItem('af:auth:access')
    if (!token) return '/start'
    const response = await fetch('/api/v1/users/me/preferences', {
      headers: { Authorization: `Bearer ${token}` },
    })
    if (!response.ok) return '/start'
    const data: { preferences?: { gettingStartedDismissed?: unknown } } = await response.json()
    return data?.preferences?.gettingStartedDismissed === true ? '/tasks' : '/start'
  } catch {
    return '/start'
  }
}

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  beforeLoad: async () => {
    throw redirect({ to: await resolveLandingPath() })
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
