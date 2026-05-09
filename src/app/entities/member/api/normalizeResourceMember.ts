import type { ResourceMember } from '../model/types'

type RawResourceMember = Partial<ResourceMember> & {
  user_id?: string
  joined_at?: string
}

export function normalizeResourceMember(raw: RawResourceMember): ResourceMember {
  const userId = raw.userId ?? raw.user_id ?? ''
  const email = raw.email ?? ''
  return {
    userId,
    email,
    username: raw.username || email.split('@')[0] || 'user',
    role: raw.role ?? 'member',
    joinedAt: raw.joinedAt ?? raw.joined_at,
  }
}
