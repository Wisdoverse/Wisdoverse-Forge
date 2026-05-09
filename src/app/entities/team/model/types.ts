export interface NavTeam {
  id: string
  orgId: string
  name: string
  slug: string
  visibility: string
  description: string
  canManage?: boolean
  canDelete?: boolean
  canCreateProject?: boolean
}

export interface CreateTeamInput {
  name: string
  slug?: string
  visibility?: string
  description?: string
}

export interface UpdateTeamInput {
  name?: string
  slug?: string
  visibility?: string
  description?: string
}
