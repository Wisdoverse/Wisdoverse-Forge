import {
  createRootRoute,
  Outlet,
  redirect,
  useRouter,
  useRouterState,
} from '@tanstack/react-router'
import { useEffect } from 'react'
import { AppLayout } from '@app/layouts/AppLayout'
import { useNavigationStore } from '@app/entities/navigation'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { useWsDispatch } from '@app/hooks/useWsDispatch'
import { useAuth } from '@app/shared/model/auth.context'
import { buildResetPasswordLoginHref, getResetTokenFromLocation } from './public-auth'

function useInitNavigation() {
  const loadOrgs = useNavigationStore((s) => s.loadOrgs)
  const loadContextFeatures = useContextFeaturesStore((s) => s.load)
  const resetContextFeatures = useContextFeaturesStore((s) => s.reset)
  const { isAuthenticated } = useAuth()

  useEffect(() => {
    if (isAuthenticated) {
      void loadOrgs()
      void loadContextFeatures()
    } else {
      resetContextFeatures()
    }
  }, [loadOrgs, loadContextFeatures, resetContextFeatures, isAuthenticated])
}

export const Route = createRootRoute({
  beforeLoad: ({ location }) => {
    // Allow /login to pass through without auth check
    if (location.pathname === '/login') return

    const resetToken = getResetTokenFromLocation(location)
    if (resetToken) {
      throw redirect({ href: buildResetPasswordLoginHref(resetToken) })
    }

    // Check auth via localStorage (synchronous, before React renders).
    // Refresh token lives in an httpOnly cookie (invisible to JS) — if the access
    // token is expired we optimistically let AuthProvider attempt a refresh, and
    // AuthProvider will redirect to /login if the cookie is missing or invalid.
    try {
      const token = localStorage.getItem('af:auth:access')
      if (!token) throw redirect({ to: '/login', search: {} })
      const parts = token.split('.')
      if (parts.length !== 3) throw redirect({ to: '/login', search: {} })
      // Access token valid or expired: either way, let AuthProvider take it from here.
    } catch (e) {
      // Re-throw redirect errors
      if (e && typeof e === 'object' && 'to' in e) throw e
      throw redirect({ to: '/login', search: {} })
    }
  },
  component: function RootLayout() {
    const router = useRouter()
    const activePath = useRouterState({ select: (s) => s.location.pathname })
    const { isLoading, isAuthenticated } = useAuth()

    useInitNavigation()
    useWsDispatch()

    // Clean up any lingering AuthPage overlay when rendering authenticated routes
    useEffect(() => {
      if (activePath !== '/login') {
        document.querySelectorAll('.auth-page, #auth-page').forEach((el) => el.remove())
      }
    }, [activePath])

    // For /login route, render Outlet without AppLayout
    if (activePath === '/login') {
      return <Outlet />
    }

    // Gate protected routes until auth completes (token refresh in progress)
    if (isLoading || !isAuthenticated) {
      return (
        <div
          style={{
            display: 'flex',
            justifyContent: 'center',
            alignItems: 'center',
            height: '100vh',
            color: '#888',
          }}
        >
          Loading…
        </div>
      )
    }

    return (
      <AppLayout activePath={activePath} onNavigate={(path) => router.navigate({ to: path })}>
        <Outlet />
      </AppLayout>
    )
  },
})
