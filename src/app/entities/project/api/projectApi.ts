import { apiFetch } from '@app/shared/api'
import {
  normalizeResourceMember,
  type AddResourceMemberInput,
  type ResourceMember,
  type UpdateResourceMemberInput,
} from '@app/entities/member'
import type {
  CloneSummary,
  CreateProjectInput,
  NavProject,
  UpdateProjectInput,
} from '../model/types'

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

export const projectApi = {
  getProjects: async (teamId: string): Promise<NavProject[]> => {
    const res = await apiFetch<{ ok: boolean; projects: NavProject[] }>(
      `/api/v1/teams/${teamId}/projects`
    )
    return res.projects
  },

  createProject: async (teamId: string, input: CreateProjectInput): Promise<NavProject> => {
    const res = await apiFetch<{ ok: boolean; project: NavProject }>(
      `/api/v1/teams/${teamId}/projects`,
      {
        method: 'POST',
        body: JSON.stringify(input),
      }
    )
    return res.project
  },

  updateProject: async (
    teamId: string,
    projectId: string,
    input: UpdateProjectInput
  ): Promise<NavProject> => {
    const res = await apiFetch<{ ok: boolean; project: NavProject }>(
      `/api/v1/teams/${teamId}/projects/${projectId}`,
      {
        method: 'PATCH',
        body: JSON.stringify(input),
      }
    )
    return res.project
  },

  deleteProject: async (teamId: string, projectId: string): Promise<void> => {
    await apiFetch<{ ok: boolean }>(`/api/v1/teams/${teamId}/projects/${projectId}`, {
      method: 'DELETE',
    })
  },

  /**
   * Retry a failed clone. Returns the new attempt's summary. The server returns
   * 409 if the latest attempt is not `failed` and 403 if the caller is not the
   * owner/manager — both surface to the caller as a thrown error with the
   * server's message.
   */
  retryClone: async (projectId: string): Promise<CloneSummary> => {
    const res = await apiFetch<{ ok: boolean; data: CloneSummary }>(
      `/api/v1/projects/${projectId}/clone/retry`,
      { method: 'POST' }
    )
    return res.data
  },

  getMembers: async (projectId: string): Promise<ResourceMember[]> => {
    const res = await apiFetch<MembersResponse>(`/api/v1/projects/${projectId}/members`)
    return (res.members ?? res.data ?? []).map((member) =>
      normalizeResourceMember(member as RawResourceMember)
    )
  },

  addMember: async (projectId: string, input: AddResourceMemberInput): Promise<ResourceMember> => {
    const res = await apiFetch<MemberResponse>(`/api/v1/projects/${projectId}/members`, {
      method: 'POST',
      body: JSON.stringify(input),
    })
    return normalizeResourceMember((res.member ?? res.data ?? {}) as RawResourceMember)
  },

  updateMember: async (
    projectId: string,
    userId: string,
    input: UpdateResourceMemberInput
  ): Promise<ResourceMember> => {
    const res = await apiFetch<MemberResponse>(`/api/v1/projects/${projectId}/members/${userId}`, {
      method: 'PATCH',
      body: JSON.stringify(input),
    })
    return normalizeResourceMember((res.member ?? res.data ?? {}) as RawResourceMember)
  },

  removeMember: async (projectId: string, userId: string): Promise<void> => {
    await apiFetch<{ ok: boolean }>(`/api/v1/projects/${projectId}/members/${userId}`, {
      method: 'DELETE',
    })
  },
}
