import { apiFetch } from '@app/shared/api'
import type { NavOrg } from '../model/types'

export const organizationApi = {
  getOrgs: async (): Promise<NavOrg[]> => {
    const res = await apiFetch<{ ok: boolean; orgs: NavOrg[] }>('/api/v1/orgs')
    return res.orgs
  },

  updateOrg: async (orgId: string, input: { name?: string }): Promise<NavOrg> => {
    const res = await apiFetch<{ ok: boolean; org: NavOrg }>(`/api/v1/orgs/${orgId}`, {
      method: 'PATCH',
      body: JSON.stringify(input),
    })
    return res.org
  },
}
