import { createRoute, redirect, useNavigate, useRouterState } from '@tanstack/react-router'
import { useEffect, useRef } from 'react'
import { Route as rootRoute } from './__root'
import { AuthPage } from '@app/features/auth'
import { useAuth } from '@app/shared/model/auth.context'
import { getResetTokenFromLocation } from './public-auth'

type LoginSearch = {
  reset_token?: string
  auth_code?: string
  auth_error?: string
  verified?: string
}

function removeAuthPageDom() {
  document.querySelectorAll('.auth-page, #auth-page').forEach((el) => el.remove())
}

function LoginPage() {
  const containerRef = useRef<HTMLDivElement>(null)
  const authPageRef = useRef<AuthPage | null>(null)
  const navigate = useNavigate()
  const { authManager } = useAuth()
  const resetToken = useRouterState({
    select: (state) => getResetTokenFromLocation(state.location),
  })
  const initialResetTokenRef = useRef<string | null>(resetToken)
  if (!initialResetTokenRef.current && resetToken) {
    initialResetTokenRef.current = resetToken
  }

  useEffect(() => {
    let cancelled = false
    const authPage = new AuthPage(authManager, 'login', initialResetTokenRef.current)
    authPageRef.current = authPage

    const authPromise = authPage.waitForAuth()

    // AuthPage appends to document.body. React StrictMode can mount, clean up,
    // then re-mount while AuthPage.show() is still awaiting provider metadata,
    // so guard stale async completions before they leave duplicate auth DOM.
    removeAuthPageDom()
    void authPage
      .show()
      .then(() => {
        if (cancelled) {
          authPage.hide()
          removeAuthPageDom()
        }
      })
      .catch((error) => {
        console.error('Failed to show auth page:', error)
      })

    void authPromise.then(() => {
      if (cancelled) return
      // Full page reload after login — cleanest way to transition from
      // vanilla JS AuthPage to React app. Navigate to root '/' which
      // serves index.html (SPA); the index route opens Tasks by default,
      // or Start only after the setup checklist is restored from Settings.
      window.location.href = '/'
    })

    return () => {
      cancelled = true
      authPage.hide()
      removeAuthPageDom()
      authPageRef.current = null
    }
  }, [authManager, navigate])

  // The AuthPage manages its own DOM (appends to body), so we just render an empty div
  return <div ref={containerRef} />
}

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login',
  validateSearch: (search: Record<string, unknown>): LoginSearch => {
    const result: LoginSearch = {}
    if (typeof search.reset_token === 'string') result.reset_token = search.reset_token
    if (typeof search.auth_code === 'string') result.auth_code = search.auth_code
    if (typeof search.auth_error === 'string') result.auth_error = search.auth_error
    if (typeof search.verified === 'string') result.verified = search.verified
    return result
  },
  beforeLoad: ({ location }) => {
    if (getResetTokenFromLocation(location)) return

    // If already authenticated, redirect to /tasks
    try {
      const token = localStorage.getItem('af:auth:access')
      if (token) {
        const parts = token.split('.')
        if (parts.length === 3) {
          const payload = JSON.parse(atob(parts[1].replace(/-/g, '+').replace(/_/g, '/')))
          if (payload.exp && payload.exp * 1000 > Date.now() + 30_000) {
            throw redirect({ to: '/tasks' })
          }
        }
      }
    } catch (e) {
      if (e && typeof e === 'object' && 'to' in e) throw e
      // Not authenticated, continue to login
    }
  },
  component: LoginPage,
})
