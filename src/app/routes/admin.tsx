import { createRoute, redirect } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { AdminPage } from '@app/pages/admin'

/**
 * Whether the cached user may enter `/admin`. Must mirror the backend
 * platform-admin gate (`AdminService::require_platform_admin`, #881), which keys
 * off the GLOBAL `users.is_admin` flag — NOT the self-assignable per-org `role`.
 *
 * `is_admin` is NOT in the JWT, so this CANNOT be derived from the token. It is
 * read from the cached user (`af:auth:user`), which `AuthManager.fetchMe()`
 * populates from `GET /me`. We gate on `isAdmin === true` and fail closed
 * otherwise (an org `owner` of their personal org is not a platform admin).
 */
export function canAccessAdmin(isAdmin: boolean | undefined): boolean {
  return isAdmin === true
}

/**
 * Read the cached `isAdmin` flag from the stored user (`af:auth:user`);
 * fail-closed on any error. The `=== true` check is load-bearing: a tampered
 * cache value such as `isAdmin: "true"` (string) or `isAdmin: 1` must NOT pass,
 * since the backend re-enforces the real gate regardless. Exported for unit
 * tests that lock these fail-closed invariants.
 */
export function storedIsAdmin(): boolean {
  try {
    const raw = localStorage.getItem('af:auth:user')
    if (!raw) return false
    const user = JSON.parse(raw) as { isAdmin?: boolean }
    return user.isAdmin === true
  } catch {
    return false
  }
}

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/admin',
  beforeLoad: () => {
    // Platform-admin guard (#881). The flag is NOT in the JWT, so read it from
    // the cached user that `/me` hydrates. Synchronous in `beforeLoad`: if the
    // guard runs before `/me` has resolved, the stored value is absent and we
    // fail closed (redirect). The backend re-enforces the gate on every
    // `/admin/*` call, so a stale-but-true value can never grant real access.
    try {
      const token = localStorage.getItem('af:auth:access')
      if (!token) throw redirect({ to: '/tasks' })
      if (!canAccessAdmin(storedIsAdmin())) throw redirect({ to: '/tasks' })
    } catch (e) {
      // Re-throw redirect errors
      if (e && typeof e === 'object' && 'to' in e) throw e
      throw redirect({ to: '/tasks' })
    }
  },
  component: AdminPage,
})
