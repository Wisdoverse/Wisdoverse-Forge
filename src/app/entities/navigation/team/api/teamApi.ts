import { apiFetch } from '@app/shared/api'
import {
  normalizeResourceMember,
  type AddResourceMemberInput,
  type ResourceMember,
  type UpdateResourceMemberInput,
} from '../../member'
import type { CreateTeamInput, NavTeam, UpdateTeamInput } from '../model/types'

type MembersResponse = {
  ok: boolean
  members?: unknown[]
  data?: unknown[]
}

type MemberResponse = {
  ok: boolean
  member?: unknown
  data?: unknown
}
type RawResourceMember = Parameters<typeof normalizeResourceMember>[0]

export const teamApi = {
  getTeams: async (orgId: string): Promise<NavTeam[]> => {
    const res = await apiFetch<{ ok: boolean; teams: NavTeam[] }>(`/api/v1/orgs/${orgId}/teams`)
    return res.teams
  },

  createTeam: async (orgId: string, input: CreateTeamInput): Promise<NavTeam> => {
    const res = await apiFetch<{ ok: boolean; team: NavTeam }>(`/api/v1/orgs/${orgId}/teams`, {
      method: 'POST',
      body: JSON.stringify(input),
    })
    return res.team
  },

  updateTeam: async (orgId: string, teamId: string, input: UpdateTeamInput): Promise<NavTeam> => {
    const res = await apiFetch<{ ok: boolean; team: NavTeam }>(
      `/api/v1/orgs/${orgId}/teams/${teamId}`,
      {
        method: 'PATCH',
        body: JSON.stringify(input),
      }
    )
    return res.team
  },

  deleteTeam: async (orgId: string, teamId: string): Promise<void> => {
    await apiFetch<{ ok: boolean }>(`/api/v1/orgs/${orgId}/teams/${teamId}`, {
      method: 'DELETE',
    })
  },

  getMembers: async (orgId: string, teamId: string): Promise<ResourceMember[]> => {
    const res = await apiFetch<MembersResponse>(`/api/v1/orgs/${orgId}/teams/${teamId}/members`)
    return (res.members ?? res.data ?? []).map((member) =>
      normalizeResourceMember(member as RawResourceMember)
    )
  },

  /** Invite a person by email: existing members are added; everyone else
   *  receives a one-time invite link (72 h) to finish signing up. */
  inviteMember: async (
    orgId: string,
    teamId: string,
    email: string,
    role?: string
  ): Promise<{ pending: boolean; inviteUrl?: string }> => {
    const res = await apiFetch<{
      ok: boolean
      pending?: boolean
      inviteUrl?: string
      member?: unknown
    }>(`/api/v1/orgs/${orgId}/teams/${teamId}/invites`, {
      method: 'POST',
      body: JSON.stringify({ email, role }),
    })
    return { pending: res.pending === true, inviteUrl: res.inviteUrl }
  },

  /** Redeem a one-time team invite with the signed-in account. */
  redeemInvite: async (token: string): Promise<{ ok: boolean }> => {
    const res = await apiFetch<{ ok: boolean }>(
      `/api/v1/invites/${encodeURIComponent(token)}/redeem`,
      {
        method: 'POST',
      }
    )
    return { ok: res.ok === true }
  },

  addMember: async (
    orgId: string,
    teamId: string,
    input: AddResourceMemberInput
  ): Promise<ResourceMember> => {
    const res = await apiFetch<MemberResponse>(`/api/v1/orgs/${orgId}/teams/${teamId}/members`, {
      method: 'POST',
      body: JSON.stringify(input),
    })
    return normalizeResourceMember((res.member ?? res.data ?? {}) as RawResourceMember)
  },

  updateMember: async (
    orgId: string,
    teamId: string,
    userId: string,
    input: UpdateResourceMemberInput
  ): Promise<ResourceMember> => {
    const res = await apiFetch<MemberResponse>(
      `/api/v1/orgs/${orgId}/teams/${teamId}/members/${userId}`,
      {
        method: 'PATCH',
        body: JSON.stringify(input),
      }
    )
    return normalizeResourceMember((res.member ?? res.data ?? {}) as RawResourceMember)
  },

  removeMember: async (orgId: string, teamId: string, userId: string): Promise<void> => {
    await apiFetch<{ ok: boolean }>(`/api/v1/orgs/${orgId}/teams/${teamId}/members/${userId}`, {
      method: 'DELETE',
    })
  },
}
