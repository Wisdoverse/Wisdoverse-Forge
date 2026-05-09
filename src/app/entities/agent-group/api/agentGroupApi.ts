import { apiFetch } from '@app/shared/api'
import type { CreateAgentGroupInput, NavAgentGroup } from '../model/types'

interface RawGroup {
  id: string
  name: string
  projectId?: string
  project_id?: string
}

function normalizeGroup(group: RawGroup, fallbackProjectId: string): NavAgentGroup {
  return {
    id: group.id,
    name: group.name,
    projectId: group.projectId ?? group.project_id ?? fallbackProjectId,
  }
}

export const agentGroupApi = {
  getGroups: async (projectId: string): Promise<NavAgentGroup[]> => {
    const res = await apiFetch<{ ok: boolean; groups: NavAgentGroup[] }>(
      `/api/v1/groups?projectId=${projectId}`
    )
    return res.groups
  },

  createGroup: async (input: CreateAgentGroupInput): Promise<NavAgentGroup> => {
    const res = await apiFetch<{ ok: boolean; group?: RawGroup | null; data?: RawGroup }>(
      '/api/v1/groups',
      {
        method: 'POST',
        body: JSON.stringify(input),
      }
    )
    const group = res.group ?? res.data
    if (!group) throw new Error('Group API did not return the created group.')
    return normalizeGroup(group, input.projectId)
  },
}
