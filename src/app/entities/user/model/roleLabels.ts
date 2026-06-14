const USER_ROLE_LABELS: Record<string, string> = {
  owner: 'Owner',
  admin: 'Admin',
  operator: 'Operator',
  maintainer: 'Maintainer',
  member: 'Member',
  viewer: 'Viewer',
  user: 'User',
}

export function userRoleLabel(role?: string | null): string {
  const normalized = role?.trim().toLowerCase()

  if (!normalized) {
    return 'Refresh access level'
  }

  return USER_ROLE_LABELS[normalized] ?? 'Access level needs review'
}
