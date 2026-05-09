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
        const body = (await res.json().catch(() => ({}))) as { error?: string }
        throw new Error(body.error ?? `HTTP ${res.status}`)
      }
      const data = (await res.json()) as { users: AdminUser[]; total: number; page: number }
      set({ users: data.users, usersTotal: data.total, usersPage: data.page, usersLoading: false })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load users'
      set({ usersLoading: false, usersError: message })
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
        const body = (await res.json().catch(() => ({}))) as { error?: string }
        throw new Error(body.error ?? `HTTP ${res.status}`)
      }
      const data = (await res.json()) as { orgs: AdminOrg[] }
      set({ orgs: data.orgs, orgsLoading: false })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load organizations'
      set({ orgsLoading: false, orgsError: message })
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
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const data = (await res.json()) as SystemHealth
      set({ health: data, healthLoading: false })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load health status'
      set({ healthLoading: false, healthError: message })
    }
  },
}))
