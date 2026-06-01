import { createRoute, redirect } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { AdminLayout } from '@app/features/admin/AdminLayout'

/**
 * Roles allowed into /admin. Must mirror the backend admin gate
 * (`AdminService::require_admin`), which accepts `owner` and `admin`. Guarding on
 * `admin` alone wrongly redirected owners away even though the API grants them
 * every admin route.
 */
export function canAccessAdmin(role: string | undefined): boolean {
  return role === 'admin' || role === 'owner'
}

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/admin',
  beforeLoad: () => {
    // Role guard: admin-only route
    // Check localStorage JWT for role claim (synchronous, before React renders)
    try {
      const token = localStorage.getItem('af:auth:access')
      if (!token) throw redirect({ to: '/tasks' })
      const parts = token.split('.')
      if (parts.length !== 3) throw redirect({ to: '/tasks' })
      const payload = JSON.parse(atob(parts[1].replace(/-/g, '+').replace(/_/g, '/'))) as {
        role?: string
      }
      if (!canAccessAdmin(payload.role)) throw redirect({ to: '/tasks' })
    } catch (e) {
      // Re-throw redirect errors
      if (e && typeof e === 'object' && 'to' in e) throw e
      throw redirect({ to: '/tasks' })
    }
  },
  component: function AdminPage() {
    return (
      <div data-testid="page-admin" className="h-full">
        <AdminLayout />
      </div>
    )
  },
})
