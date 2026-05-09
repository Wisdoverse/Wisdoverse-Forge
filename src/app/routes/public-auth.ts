type ResetTokenLocation = {
  href?: string
  search?: unknown
  searchStr?: string
}

function nonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0
}

export function getResetTokenFromLocation(location: ResetTokenLocation): string | null {
  if (location.searchStr) {
    const token = new URLSearchParams(location.searchStr).get('reset_token')
    if (nonEmptyString(token)) return token
  }

  if (location.href) {
    const query = location.href.includes('?') ? location.href.slice(location.href.indexOf('?')) : ''
    const token = new URLSearchParams(query).get('reset_token')
    if (nonEmptyString(token)) return token
  }

  if (location.search && typeof location.search === 'object' && !Array.isArray(location.search)) {
    const value = (location.search as Record<string, unknown>).reset_token
    if (nonEmptyString(value)) return value
  }

  return null
}

export function buildResetPasswordLoginHref(resetToken: string): string {
  return `/login?reset_token=${encodeURIComponent(resetToken)}`
}
