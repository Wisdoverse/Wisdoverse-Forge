export type WorkspaceSettingsResource = 'team' | 'project'
export type WorkspaceSettingsAction = 'load' | 'create'

function rawErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function statusCode(err: unknown): number | null {
  const match = rawErrorMessage(err).match(/\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function errorDetail(err: unknown): string {
  const message = rawErrorMessage(err).trim()
  const match = /\b(?:HTTP|API|Server error|Code:)\s*\(?\d{3}\)?\s*:?\s*(.*)$/is.exec(message)
  const detail = match?.[1]?.trim() ?? ''
  if (!detail) return ''

  try {
    const parsed = JSON.parse(detail) as unknown
    if (parsed && typeof parsed === 'object') {
      const data = parsed as Record<string, unknown>
      for (const key of ['error', 'message', 'detail']) {
        const value = data[key]
        if (typeof value === 'string' && value.trim()) return value.trim()
      }
    }
  } catch {
    // Keep a short server-provided detail below when it was not JSON.
  }

  return detail
}

function isNetworkError(err: unknown): boolean {
  const text = rawErrorMessage(err).toLowerCase()
  return (
    err instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('load failed')
  )
}

function resourceLabel(resource: WorkspaceSettingsResource): string {
  return resource === 'team' ? 'team' : 'project'
}

function baseMessage(resource: WorkspaceSettingsResource, action: WorkspaceSettingsAction): string {
  const label = resourceLabel(resource)
  return action === 'load'
    ? `Workspace ${label}s could not be loaded.`
    : `The ${label} was not created.`
}

export function workspaceSettingsErrorMessage(
  resource: WorkspaceSettingsResource,
  action: WorkspaceSettingsAction,
  err: unknown
): string {
  const base = baseMessage(resource, action)
  const text = rawErrorMessage(err).toLowerCase()
  const detail = errorDetail(err).toLowerCase()
  const code = statusCode(err)

  if (code === 401 || text.includes('unauthorized')) {
    return `${base} Sign in again, then return to Settings.`
  }
  if (code === 403 || text.includes('permission') || text.includes('forbidden')) {
    return `${base} Ask an owner or admin to update your workspace access.`
  }
  if (code === 404 || text.includes('endpoint is not available')) {
    return `${base} Refresh Settings; the organization, team, or project may have changed.`
  }
  if (code === 409 || text.includes('already exists')) {
    return action === 'create'
      ? `${base} Use a different name, then try again.`
      : `${base} Another setup change is still saving. Wait a moment, then try again.`
  }
  if (code === 422 || text.includes('invalid')) {
    if (detail.includes('name')) {
      return `${base} Enter a ${resourceLabel(resource)} name, then try again.`
    }
    return `${base} Check the name and required fields, then try again.`
  }
  if (code === 429 || text.includes('busy') || text.includes('too many')) {
    return `${base} Too many setup changes are happening right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The workspace settings service is temporarily unavailable. Ask an owner or admin to check the backend, then refresh Settings.`
  }
  if (isNetworkError(err)) {
    return `${base} Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check the workspace setup.`
}
