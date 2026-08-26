import { createRouter, createRoute, RouterProvider, useNavigate } from '@tanstack/react-router'
import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { useSettingsStore } from '@app/entities/settings'
import { Route as rootRoute } from './routes/__root'
import { Route as loginRoute } from './routes/login'
import { Route as startRoute } from './routes/start'
import { Route as tasksRoute, DetailRoute as tasksDetailRoute } from './routes/tasks'
import { Route as inboxRoute } from './routes/inbox'
import { Route as contextRoute } from './routes/context'
import { Route as contextAuditRoute } from './routes/context-audit'
import { Route as agentsRoute } from './routes/agents'
import { Route as skillsRoute } from './routes/skills'
import { Route as settingsRoute, SectionRoute as settingsSectionRoute } from './routes/settings'
import { Route as billingRoute } from './routes/billing'
import { Route as adminRoute } from './routes/admin'
import { Route as analyticsRoute } from './routes/analytics'
import { Route as operationsRoute } from './routes/operations'
import { RouteErrorFallback } from './shared/ui/RouteErrorFallback'

/**
 * Landing for the app root.
 *
 * New workspaces should start on the setup checklist (/start) so the first
 * session is guided instead of landing on an empty board. The "dismissed"
 * preference is the source of truth: it flips to true when a user skips the
 * guide or completes every step, so returning users go straight to the board.
 * If preferences cannot be loaded, we fall back to the board (the previous
 * behavior) rather than blocking the first screen.
 */
function IndexLanding() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const loadPreferences = useSettingsStore((s) => s.loadPreferences)
  const decidedRef = useRef(false)

  useEffect(() => {
    // Preferences may already be loaded (e.g. an earlier route warmed the
    // store in this session); decide immediately instead of waiting on a
    // subscription transition that will never fire.
    if (useSettingsStore.getState().preferencesLoaded) {
      decidedRef.current = true
      const dismissed = useSettingsStore.getState().preferences?.gettingStartedDismissed === true
      void navigate({ to: dismissed ? '/tasks' : '/start' })
      return
    }
    let cancelled = false
    void loadPreferences()
    const unsubscribe = useSettingsStore.subscribe((state, prevState) => {
      if (decidedRef.current || cancelled) return
      if (state.preferencesLoaded && !prevState.preferencesLoaded) {
        decidedRef.current = true
        unsubscribe()
        const go = state.preferences?.gettingStartedDismissed === true ? '/tasks' : '/start'
        void navigate({ to: go })
      }
    })
    const fallback = window.setTimeout(() => {
      if (decidedRef.current || cancelled) return
      const state = useSettingsStore.getState()
      if (!state.preferencesLoaded) {
        decidedRef.current = true
        unsubscribe()
        void navigate({ to: '/tasks' })
      }
    }, 8000)
    return () => {
      cancelled = true
      unsubscribe()
      window.clearTimeout(fallback)
    }
  }, [loadPreferences, navigate])

  return (
    <div
      role="status"
      aria-live="polite"
      data-testid="landing-opening"
      className="flex h-full min-h-64 items-center justify-center px-4"
    >
      <div className="flex max-w-sm flex-col items-center gap-2 text-center">
        <div className="h-2 w-2 animate-pulse rounded-full bg-apple-blue" aria-hidden="true" />
        <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          {t('appLanding.opening')}
        </p>
      </div>
    </div>
  )
}

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: IndexLanding,
})

const routeTree = rootRoute.addChildren([
  indexRoute,
  loginRoute,
  startRoute,
  tasksRoute,
  tasksDetailRoute,
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
  operationsRoute,
])

const router = createRouter({ routeTree, defaultErrorComponent: RouteErrorFallback })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

export default function App() {
  return <RouterProvider router={router} />
}
