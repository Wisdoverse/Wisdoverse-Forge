export type WorkspaceResourceKind = 'team' | 'project'
export type WorkspaceResourceAction = 'update' | 'delete'

const RAW_NETWORK_ERRORS = [/^Network error$/i, /^Failed to fetch$/i]
const RAW_STATUS_ERRORS = [/^API\s+\d{3}/i, /^HTTP\s+\d{3}/i, /^Server error\s*\(\d{3}\)$/i]
const GENERIC_BODY_TEXT = /^(Unauthorized|Forbidden|Not Found|Internal Server Error)$/i

export function workspaceResourceErrorMessage(
  resource: WorkspaceResourceKind,
  action: WorkspaceResourceAction,
  error?: unknown
): string {
  const status = statusFromError(error)
  const detail = status === null || status === 400 || status === 422 ? safeDetail(error) : null

  if (!status) {
    if (detail) {
      return validationMessage(resource, action, detail)
    }
    return `${label(resource)} could not ${actionPhrase(action)} because the app could not reach the service. Check your connection, then try again.`
  }

  if (status === 401) {
    return `Sign in again, then reopen Settings and ${retryPhrase(resource, action)}.`
  }
  if (status === 403) {
    return `You do not have permission to ${permissionAction(action)} this ${resource}. Ask an owner or admin to update your role.`
  }
  if (status === 404) {
    return `This ${resource} could not be found. Refresh Settings, then choose an existing ${resource}.`
  }
  if (status === 409) {
    return `This ${resource} changed while you were editing. Refresh Settings, review the current ${resource}, then try again.`
  }
  if (status === 400 || status === 422) {
    return validationMessage(resource, action, detail)
  }
  if (status === 429) {
    return `Workspace settings are busy. Wait a moment, then ${retryPhrase(resource, action)}.`
  }
  if (status >= 500) {
    return 'Workspace settings are temporarily unavailable. Refresh Settings, then try again. If it still fails, ask an owner or admin to check workspace setup.'
  }

  return `${label(resource)} could not ${actionPhrase(action)}. Refresh Settings and try again.`
}

function label(resource: WorkspaceResourceKind): string {
  return resource === 'team' ? 'Team' : 'Project'
}

function actionPhrase(action: WorkspaceResourceAction): string {
  return action === 'update' ? 'be saved' : 'be deleted'
}

function permissionAction(action: WorkspaceResourceAction): string {
  return action === 'update' ? 'save' : 'delete'
}

function retryPhrase(resource: WorkspaceResourceKind, action: WorkspaceResourceAction): string {
  if (action === 'update') return `save the ${resource} again`
  return `delete the ${resource} again`
}

function validationMessage(
  resource: WorkspaceResourceKind,
  action: WorkspaceResourceAction,
  detail?: string | null
): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (action === 'update') {
    if (normalized.includes('name')) {
      return resource === 'team'
        ? 'Enter a team name, then save again.'
        : 'Enter a project name, then save again.'
    }
    return resource === 'team'
      ? 'Check the team name and description, then save again.'
      : 'Check the project name, description, and color, then save again.'
  }
  if (resource === 'team' && normalized.includes('project')) {
    return "Move or delete this team's projects first, then delete the team again."
  }
  if (resource === 'project' && normalized.includes('agent')) {
    return 'Move agents out of this project first, then delete the project again.'
  }
  if (resource === 'project' && normalized.includes('task')) {
    return "Move or finish this project's tasks first, then delete the project again."
  }
  return resource === 'team'
    ? 'This team could not be deleted. Check whether it still owns projects or required access, then try again.'
    : 'This project could not be deleted. Check whether agents or tasks still depend on it, then try again.'
}

function statusFromError(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const status = (error as { status?: unknown }).status
    if (typeof status === 'number') return status

    const statusCode = (error as { statusCode?: unknown }).statusCode
    if (typeof statusCode === 'number') return statusCode
  }

  const detail = rawDetail(error)
  const match = detail?.match(/\b(?:API|HTTP|Server error\s*\()? ?(\d{3})\b/i)
  return match ? Number(match[1]) : null
}

function rawDetail(error: unknown): string | null {
  if (typeof error === 'string' && error.trim()) return error.trim()
  if (error instanceof Error && error.message.trim()) return error.message.trim()
  if (error && typeof error === 'object') {
    const message = (error as { message?: unknown }).message
    if (typeof message === 'string' && message.trim()) return message.trim()

    const errorValue = (error as { error?: unknown }).error
    if (typeof errorValue === 'string' && errorValue.trim()) return errorValue.trim()
  }
  return null
}

function safeDetail(error: unknown): string | null {
  const detail = rawDetail(error)
  if (!detail) return null
  if (RAW_NETWORK_ERRORS.some((pattern) => pattern.test(detail))) return null

  const statusBody = detail.match(/^(?:API|HTTP)\s+\d{3}:?\s*(.*)$/i)
  if (statusBody) return safeBodyDetail(statusBody[1])

  if (RAW_STATUS_ERRORS.some((pattern) => pattern.test(detail))) return null
  return trimDetail(detail)
}

function safeBodyDetail(body: string): string | null {
  const trimmed = body.trim()
  if (!trimmed || GENERIC_BODY_TEXT.test(trimmed)) return null

  const parsed = parseJsonBody(trimmed)
  const payloadDetail = parsed ? firstPayloadString(parsed) : null
  return trimDetail(payloadDetail ?? trimmed)
}

function parseJsonBody(body: string): unknown | null {
  try {
    return JSON.parse(body)
  } catch {
    return null
  }
}

function firstPayloadString(value: unknown): string | null {
  if (typeof value === 'string' && value.trim()) return value.trim()
  if (Array.isArray(value)) {
    for (const item of value) {
      const found = firstPayloadString(item)
      if (found) return found
    }
    return null
  }
  if (!value || typeof value !== 'object') return null

  const record = value as Record<string, unknown>
  for (const key of ['message', 'error', 'detail', 'reason']) {
    const found = firstPayloadString(record[key])
    if (found) return found
  }
  return null
}

function trimDetail(detail: string | null): string | null {
  const trimmed = detail?.trim()
  if (!trimmed || RAW_NETWORK_ERRORS.some((pattern) => pattern.test(trimmed))) return null
  if (GENERIC_BODY_TEXT.test(trimmed)) return null
  return trimmed.length > 180 ? `${trimmed.slice(0, 177)}...` : trimmed
}
