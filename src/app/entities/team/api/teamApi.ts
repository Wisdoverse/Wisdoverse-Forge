import { apiFetch } from '@app/shared/api'
import {
  normalizeResourceMember,
  type AddResourceMemberInput,
  type ResourceMember,
  type UpdateResourceMemberInput,
} from '@app/entities/member'
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
