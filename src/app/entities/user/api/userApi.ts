import { apiFetch } from '@app/shared/api'
import type { OrgUser } from '../model/types'

type ApiUser = {
  id?: string
  userId?: string
  user_id?: string
  email?: string
  username?: string
  displayName?: string | null
  display_name?: string | null
  role?: string
}

function normalizeOrgUser(raw: ApiUser): OrgUser {
  const email = raw.email ?? ''
  return {
    id: raw.id ?? raw.userId ?? raw.user_id ?? '',
    email,
    username: raw.username || raw.displayName || raw.display_name || email.split('@')[0] || 'user',
    role: raw.role,
  }
}

export const userApi = {
  getUsers: async (limit = 100): Promise<OrgUser[]> => {
    const res = await apiFetch<{ ok: boolean; data?: ApiUser[]; users?: ApiUser[] }>(
      `/api/v1/users?limit=${limit}`
    )
    return (res.data ?? res.users ?? []).map(normalizeOrgUser).filter((user) => user.id)
  },

  searchUsers: async (query: string, limit = 20): Promise<OrgUser[]> => {
    const params = new URLSearchParams({ q: query, limit: String(limit) })
    const res = await apiFetch<{ ok: boolean; members?: ApiUser[]; data?: ApiUser[] }>(
      `/api/v1/users/search?${params.toString()}`
    )
    return (res.members ?? res.data ?? []).map(normalizeOrgUser).filter((user) => user.id)
  },
}
