import { createRoute, redirect } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { LoginPage } from '@app/pages/login'
import { getResetTokenFromLocation } from '@app/shared/lib/publicAuth'

type LoginSearch = {
  reset_token?: string
  auth_code?: string
  auth_error?: string
  verified?: string
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
