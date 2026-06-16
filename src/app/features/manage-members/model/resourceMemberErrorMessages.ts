export type ResourceMemberErrorAction = 'load' | 'add' | 'updateRole' | 'remove'
export type ResourceMemberResourceLabel = 'Team' | 'Project'

const RAW_NETWORK_ERRORS = [/^Network error$/i, /^Failed to fetch$/i]
const RAW_STATUS_ERRORS = [/^API\s+\d{3}/i, /^HTTP\s+\d{3}/i, /^Server error\s*\(\d{3}\)$/i]
const GENERIC_BODY_TEXT = /^(Unauthorized|Forbidden|Not Found|Internal Server Error)$/i

export function resourceMemberErrorMessage(
  action: ResourceMemberErrorAction,
  resourceLabel: ResourceMemberResourceLabel,
  error?: unknown
): string {
  const status = statusFromResourceMemberError(error)
  const detail = status === null || status === 422 ? safeDetailFromResourceMemberError(error) : null
  const resource = resourceLabel.toLowerCase()

  if (!status) {
    if (detail) {
      return validationMessage(action, resource, detail)
    }
    return memberConnectionMessage(action, resource)
  }

  if (status === 401) {
    return `Sign in again, then reopen members for this ${resource}.`
  }
  if (status === 403) {
    return `Ask an owner or admin to give you access to manage people here, then reopen members for this ${resource}. You do not have permission right now.`
  }
  if (status === 404) {
    return `Refresh members or choose another ${resource}. People for this ${resource} are not available.`
  }
  if (status === 409) {
    return "This person's access changed while you were editing. Refresh the members list, review who has access, then try again."
  }
  if (status === 422) {
    return validationMessage(action, resource, detail)
  }
  if (status === 429) {
    return `Wait a moment, then ${retrySummary(action, resource)}. People access is busy right now.`
  }
  if (status >= 500) {
    return memberUnavailableMessage(action, resource)
  }

  return `Refresh the members list, then ${retrySummary(action, resource)}. Forge could not ${actionSummary(action, resource)}.`
}

function memberConnectionMessage(action: ResourceMemberErrorAction, resource: string): string {
  if (action === 'load') {
    return `Check your connection, then reopen members for this ${resource}.`
  }
  const operation = 'updating people access'
  return `Check your connection, then ${retrySummary(action, resource)}. Forge could not connect while ${operation}.`
}

function memberUnavailableMessage(action: ResourceMemberErrorAction, resource: string): string {
  if (action === 'load') {
    return `Refresh members to load people for this ${resource}. If it still fails, ask an owner or admin to check people access settings.`
  }
  const operation = 'update people access'
  return `Refresh members, then ${retrySummary(action, resource)}. Forge could not ${operation} right now. If it still fails, ask an owner or admin to check people access settings.`
}

function actionSummary(action: ResourceMemberErrorAction, resource: string): string {
  switch (action) {
    case 'load':
      return `load people for this ${resource}`
    case 'add':
      return `add this person to this ${resource}`
    case 'updateRole':
      return `change what this person can do on this ${resource}`
    case 'remove':
      return `remove this person from this ${resource}`
  }
}

function retrySummary(action: ResourceMemberErrorAction, resource: string): string {
  switch (action) {
    case 'load':
      return `reopen members for this ${resource}`
    case 'add':
      return `add the person again`
    case 'updateRole':
      return `save the access change again`
    case 'remove':
      return `remove the person again`
  }
}

function validationMessage(
  action: ResourceMemberErrorAction,
  resource: string,
  detail?: string | null
): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes(`no ${resource} selected`)) {
    return `This ${resource} is no longer selected. Close members, choose the ${resource} again, then add or change people.`
  }

  switch (action) {
    case 'load':
      return `Refresh members to load people for this ${resource}.`
    case 'add':
      if (normalized.includes('role')) {
        return 'Choose this person and what they can do, then add them again.'
      }
      return `Check the selected person and what they can do, then add them again.`
    case 'updateRole':
      if (normalized.includes('owner')) {
        return `Choose a different owner first, then change what this person can do on this ${resource}.`
      }
      return `Check what this person can do, then save the change again.`
    case 'remove':
      if (normalized.includes('owner')) {
        return `Choose a different owner first, then remove this person from this ${resource}.`
      }
      return `Check whether this person is the last owner or still required for this ${resource}, then try removing them again. This person could not be removed.`
  }
}

function statusFromResourceMemberError(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const status = (error as { status?: unknown }).status
    const parsedStatus = numericStatus(status)
    if (parsedStatus) return parsedStatus

    const statusCode = (error as { statusCode?: unknown }).statusCode
    const parsedStatusCode = numericStatus(statusCode)
    if (parsedStatusCode) return parsedStatusCode
  }

  const detail = rawDetailFromResourceMemberError(error)
  const match = detail?.match(/\b(?:API|HTTP|Server error\s*\()? ?(\d{3})\b/i)
  return match ? Number(match[1]) : null
}

function numericStatus(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && /^\d+$/.test(value)) return Number(value)
  return null
}

function rawDetailFromResourceMemberError(error: unknown): string | null {
  if (typeof error === 'string' && error.trim()) return error.trim()
  if (error instanceof Error && error.message.trim()) return error.message.trim()
  if (error && typeof error === 'object') {
    const record = error as {
      serverError?: unknown
      detail?: unknown
      error?: unknown
      message?: unknown
      reason?: unknown
    }

    for (const candidate of [
      record.serverError,
      record.detail,
      record.error,
      record.message,
      record.reason,
    ]) {
      if (typeof candidate === 'string' && candidate.trim()) return candidate.trim()
    }
  }
  return null
}

function safeDetailFromResourceMemberError(error: unknown): string | null {
  const rawDetail = rawDetailFromResourceMemberError(error)
  if (!rawDetail) return null
  if (RAW_NETWORK_ERRORS.some((pattern) => pattern.test(rawDetail))) return null

  const statusBody = rawDetail.match(/^(?:API|HTTP)\s+\d{3}:?\s*(.*)$/i)
  if (statusBody) return safeBodyDetail(statusBody[1])

  if (RAW_STATUS_ERRORS.some((pattern) => pattern.test(rawDetail))) return null
  return trimDetail(rawDetail)
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
  for (const key of ['serverError', 'message', 'error', 'detail', 'reason']) {
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
