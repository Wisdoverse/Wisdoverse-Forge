import { createRoute, redirect } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { AdminLayout } from '@app/features/admin/AdminLayout'

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
      if (payload.role !== 'admin') throw redirect({ to: '/tasks' })
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
