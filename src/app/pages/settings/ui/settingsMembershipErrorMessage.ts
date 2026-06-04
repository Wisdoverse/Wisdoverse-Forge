type MembershipResource = 'teams' | 'projects'
type MembershipAction = 'load' | 'create'

interface MembershipErrorOptions {
  resource: MembershipResource
  action: MembershipAction
}

const RESOURCE_LABEL: Record<MembershipResource, string> = {
  teams: 'Teams',
  projects: 'Projects',
}

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  return typeof error === 'string' ? error : ''
}

function statusCode(error: unknown): number | null {
  const match = errorText(error).match(/\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(error: unknown): boolean {
  const text = errorText(error).toLowerCase()
  return (
    error instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('browser could not reach') ||
    text.includes('load failed')
  )
}

function baseMessage({ action, resource }: MembershipErrorOptions): string {
  const label = RESOURCE_LABEL[resource]
  return action === 'load' ? `${label} could not be loaded.` : `${label} could not be created.`
}

export function settingsMembershipErrorMessage(
  error: unknown,
  options: MembershipErrorOptions
): string {
  const base = baseMessage(options)
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)
  const target = options.resource === 'teams' ? 'teams' : 'projects'

  if (code === 401 || text.includes('sign in again') || text.includes('unauthorized')) {
    return `${base} Sign in again, then open Settings and try ${target} again.`
  }
  if (code === 403 || text.includes('permission') || text.includes('forbidden')) {
    return `${base} Ask an owner or admin for access to manage ${target}.`
  }
  if (code === 404 || text.includes('endpoint is not available')) {
    return `${base} Refresh after the workspace settings service is available.`
  }
  if (options.action === 'create' && (code === 409 || text.includes('already exists'))) {
    return `${base} Use a different name, then try again.`
  }
  if (options.action === 'create' && (code === 422 || text.includes('invalid'))) {
    return `${base} Check the name, then try again.`
  }
  if (code === 429 || text.includes('busy') || text.includes('too many')) {
    return `${base} The server is busy. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The workspace settings service is temporarily unavailable. Ask an owner to check the backend, then try again.`
  }
  if (isNetworkError(error)) {
    return `${base} The browser could not reach the server. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner to check workspace settings.`
}
