export type ResourceMemberRole = 'owner' | 'admin' | 'maintainer' | 'member'

export interface ResourceMember {
  userId: string
  email: string
  username: string
  role: ResourceMemberRole | string
  joinedAt?: string
}

export interface AddResourceMemberInput {
  userId: string
  role: ResourceMemberRole
}

export interface UpdateResourceMemberInput {
  role: ResourceMemberRole
}
