export interface NavProject {
  id: string
  teamId: string
  workspaceId?: string
  name: string
  slug: string
  color: string
  description: string
  canManage?: boolean
  canDelete?: boolean
}

export interface CreateProjectInput {
  name: string
  slug?: string
  color?: string
  description?: string
}

export interface UpdateProjectInput {
  name?: string
  slug?: string
  color?: string
  description?: string
}
