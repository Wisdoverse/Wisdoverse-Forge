export type WorkspaceSettingsResource = 'team' | 'project'
export type WorkspaceSettingsAction = 'load' | 'create'

function rawErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function statusCode(err: unknown): number | null {
  const match = rawErrorMessage(err).match(/\b(?:HTTP|API)\s+(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
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
  const code = statusCode(err)

  if (code === 401) {
    return `${base} Sign in again, then return to Settings.`
  }
  if (code === 403) {
    return `${base} Ask an owner or admin to update your workspace access.`
  }
  if (code === 404) {
    return `${base} Refresh the page; the organization, team, or project may have changed.`
  }
  if (code === 409) {
    return `${base} Another setup change is still saving. Wait a moment, then try again.`
  }
  if (code === 422) {
    return `${base} Check the name and required fields, then try again.`
  }
  if (code === 429) {
    return `${base} Too many setup changes are happening right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The platform is temporarily unavailable. Try again in a few minutes.`
  }
  if (isNetworkError(err)) {
    return `${base} Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check the workspace setup.`
}
