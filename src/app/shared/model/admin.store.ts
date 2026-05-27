import { create } from 'zustand'
import { getAuthFetch } from '@app/shared/api/legacy'

// ============================================================================
// Types
// ============================================================================

export type AdminSection = 'users' | 'organizations' | 'health'

export interface AdminUser {
  id: string
  email: string
  displayName: string
  role: string
  status: 'active' | 'inactive'
  createdAt: string | null
  lastLoginAt: string | null
  sessionsCount: number
}

export interface AdminOrg {
  id: string
  name: string
  slug: string
  plan: string
  membersCount: number
  teamsCount: number
  createdAt: string
}

export interface ComponentHealth {
  status: 'up' | 'down' | 'degraded'
  latencyMs?: number
  error?: string
}

export interface SystemHealth {
  status: 'healthy' | 'degraded' | 'unhealthy'
  checks: {
    database?: ComponentHealth & { pool?: { total: number; idle: number; waiting: number } }
    redis?: ComponentHealth & { mode?: string; circuitState?: string }
    nats?: ComponentHealth
    platform?: ComponentHealth & { version?: string; uptime?: number }
    bullmq?: ComponentHealth
  }
  uptime?: number
  version?: string
}

interface AdminState {
  // Navigation
  activeSection: AdminSection

  // Users
  users: AdminUser[]
  usersTotal: number
  usersPage: number
  usersLoading: boolean
  usersError: string | null
  userSearch: string

  // Orgs
  orgs: AdminOrg[]
  orgsLoading: boolean
  orgsError: string | null

  // Health
  health: SystemHealth | null
  healthLoading: boolean
  healthError: string | null

  // Actions
  setActiveSection: (section: AdminSection) => void
  setUserSearch: (search: string) => void

  loadUsers: (page?: number) => Promise<void>
  updateUserRole: (id: string, role: string) => Promise<boolean>
  deleteUser: (id: string) => Promise<boolean>

  loadOrgs: () => Promise<void>

  loadHealth: () => Promise<void>
}

type AdminResource = 'users' | 'organizations' | 'health'

class AdminUserFacingError extends Error {}

function userFacingError(message: string): AdminUserFacingError {
  return new AdminUserFacingError(message)
}

// ============================================================================
// Helpers
// ============================================================================

/**
 * Admin fetch using the legacy API's authFetch for consistent 401 refresh.
 */
async function adminFetch(path: string, init?: RequestInit): Promise<Response> {
  const authFetch = getAuthFetch()
  return authFetch(path, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers as Record<string, string> | undefined),
    },
  })
}

function adminResourceLabel(resource: AdminResource): string {
  switch (resource) {
    case 'users':
      return 'user list'
    case 'organizations':
      return 'organization list'
    case 'health':
      return 'system health'
  }
}

function adminErrorDetail(data: Record<string, unknown>): string | null {
  if (typeof data.error === 'string' && data.error.trim()) return data.error.trim()
  if (
    data.error &&
    typeof data.error === 'object' &&
    'message' in data.error &&
    typeof data.error.message === 'string' &&
    data.error.message.trim()
  ) {
    return data.error.message.trim()
  }
  if (typeof data.message === 'string' && data.message.trim()) return data.message.trim()
  return null
}

async function readAdminErrorPayload(res: Response): Promise<Record<string, unknown>> {
  return ((await res.json().catch(() => ({}))) ?? {}) as Record<string, unknown>
}

export function adminHttpErrorMessage(
  resource: AdminResource,
  status: number,
  data: Record<string, unknown> = {}
): string {
  const label = adminResourceLabel(resource)
  const detail = adminErrorDetail(data)
  const suffix = detail ? ` Details: ${detail}` : ''
  const statusText = `Code: ${status}.`

  if (status === 401) {
    return `Sign in again, then reload the ${label}. ${statusText}${suffix}`
  }
  if (status === 403) {
    return `You do not have permission to view admin ${label}. Ask an owner to update your admin role. ${statusText}${suffix}`
  }
  if (status === 404) {
    return `The admin ${label} endpoint is not available. Refresh after the backend is deployed. ${statusText}${suffix}`
  }
  if (status === 429) {
    return `The admin service is busy. Wait a moment, then reload the ${label}. ${statusText}${suffix}`
  }
  if (status >= 500) {
    return `The admin service had a server problem. Try again after the backend is healthy. ${statusText}${suffix}`
  }

  return `The admin ${label} could not load. Refresh the page and try again. ${statusText}${suffix}`
}

function adminNetworkErrorMessage(resource: AdminResource): string {
  return `The admin ${adminResourceLabel(resource)} could not load because the browser could not reach the server. Check your connection and refresh the page.`
}

function adminErrorMessage(err: unknown, resource: AdminResource): string {
  return err instanceof AdminUserFacingError ? err.message : adminNetworkErrorMessage(resource)
}

// ============================================================================
// Store
// ============================================================================

export const useAdminStore = create<AdminState>((set, get) => ({
  activeSection: 'users',

  users: [],
  usersTotal: 0,
  usersPage: 1,
  usersLoading: false,
  usersError: null,
  userSearch: '',

  orgs: [],
  orgsLoading: false,
  orgsError: null,

  health: null,
  healthLoading: false,
  healthError: null,

  setActiveSection: (activeSection) => set({ activeSection }),
  setUserSearch: (userSearch) => set({ userSearch }),

  // ---------------------------------------------------------------------------
  // Users
  // ---------------------------------------------------------------------------

  loadUsers: async (page = 1) => {
    const { userSearch } = get()
    set({ usersLoading: true, usersError: null })
    try {
      const params = new URLSearchParams({ page: String(page), limit: '25' })
      if (userSearch) params.set('search', userSearch)
      const res = await adminFetch(`/api/v1/admin/users?${params.toString()}`)
      if (!res.ok) {
        throw userFacingError(
          adminHttpErrorMessage('users', res.status, await readAdminErrorPayload(res))
        )
      }
      const data = (await res.json()) as { users: AdminUser[]; total: number; page: number }
      set({ users: data.users, usersTotal: data.total, usersPage: data.page, usersLoading: false })
    } catch (err) {
      set({ usersLoading: false, usersError: adminErrorMessage(err, 'users') })
    }
  },

  updateUserRole: async (id, role) => {
    try {
      const res = await adminFetch(`/api/v1/admin/users/${id}`, {
        method: 'PUT',
        body: JSON.stringify({ role }),
      })
      if (!res.ok) return false
      set((state) => ({
        users: state.users.map((u) => (u.id === id ? { ...u, role } : u)),
      }))
      return true
    } catch {
      return false
    }
  },

  deleteUser: async (id) => {
    try {
      const res = await adminFetch(`/api/v1/admin/users/${id}`, { method: 'DELETE' })
      if (!res.ok) return false
      set((state) => ({ users: state.users.filter((u) => u.id !== id) }))
      return true
    } catch {
      return false
    }
  },

  // ---------------------------------------------------------------------------
  // Orgs
  // ---------------------------------------------------------------------------

  loadOrgs: async () => {
    set({ orgsLoading: true, orgsError: null })
    try {
      const res = await adminFetch('/api/v1/admin/orgs')
      if (!res.ok) {
        throw userFacingError(
          adminHttpErrorMessage('organizations', res.status, await readAdminErrorPayload(res))
        )
      }
      const data = (await res.json()) as { orgs: AdminOrg[] }
      set({ orgs: data.orgs, orgsLoading: false })
    } catch (err) {
      set({ orgsLoading: false, orgsError: adminErrorMessage(err, 'organizations') })
    }
  },

  // ---------------------------------------------------------------------------
  // Health
  // ---------------------------------------------------------------------------

  loadHealth: async () => {
    set({ healthLoading: true, healthError: null })
    try {
      // Use admin-authed call to get detailed health info
      const res = await adminFetch('/api/v1/health')
      if (!res.ok) {
        throw userFacingError(
          adminHttpErrorMessage('health', res.status, await readAdminErrorPayload(res))
        )
      }
      const data = (await res.json()) as SystemHealth
      set({ health: data, healthLoading: false })
    } catch (err) {
      set({ healthLoading: false, healthError: adminErrorMessage(err, 'health') })
    }
  },
}))
