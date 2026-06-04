export type AccountErrorAction = 'changePassword' | 'renameOrganization'

const RAW_NETWORK_ERRORS = [/^Network error$/i, /^Failed to fetch$/i]
const RAW_STATUS_ERRORS = [/^API\s+\d{3}/i, /^HTTP\s+\d{3}/i, /^Server error\s*\(\d{3}\)$/i]
const GENERIC_BODY_TEXT = /^(Unauthorized|Forbidden|Not Found|Internal Server Error)$/i

export function accountErrorMessage(action: AccountErrorAction, error?: unknown): string {
  const status = statusFromAccountError(error)
  const detail = shouldUseDetail(status) ? safeDetailFromAccountError(error) : null

  if (!status) {
    if (detail) {
      return validationMessage(action, detail)
    }
    return `Account settings could not ${actionPhrase(action)} because the browser could not reach the server. Check your connection, then try again.`
  }

  if (status === 401) {
    return `Sign in again, then ${retryPhrase(action)}.`
  }
  if (status === 403) {
    return permissionMessage(action)
  }
  if (status === 404) {
    return `${serviceLabel(action)} is not available. Refresh after the backend is deployed, then try again.`
  }
  if (status === 409) {
    return conflictMessage(action)
  }
  if (status === 422 || status === 400) {
    return validationMessage(action, detail)
  }
  if (status === 429) {
    return `Account settings are busy. Wait a moment, then ${retryPhrase(action)}.`
  }
  if (status >= 500) {
    return `${serviceLabel(action)} had a server problem. Try again after the backend is healthy.`
  }

  return `Account settings could not ${actionPhrase(action)}. Refresh the page and try again.`
}

function actionPhrase(action: AccountErrorAction): string {
  return action === 'changePassword' ? 'change your password' : 'rename the organization'
}

function retryPhrase(action: AccountErrorAction): string {
  return action === 'changePassword'
    ? 'change your password again'
    : 'rename the organization again'
}

function serviceLabel(action: AccountErrorAction): string {
  return action === 'changePassword' ? 'The password service' : 'The organization settings service'
}

function permissionMessage(action: AccountErrorAction): string {
  if (action === 'changePassword') {
    return 'You do not have permission to change this password. Ask an owner or admin to check your account.'
  }
  return 'You do not have permission to rename this organization. Ask an owner or admin to update your role.'
}

function conflictMessage(action: AccountErrorAction): string {
  if (action === 'changePassword') {
    return 'Your account changed while this form was open. Refresh the page, then try again.'
  }
  return 'This organization changed while you were editing. Refresh organization settings, review the current name, then try again.'
}

function validationMessage(action: AccountErrorAction, detail?: string | null): string {
  const normalizedDetail = detail?.toLowerCase() ?? ''
  if (action === 'changePassword') {
    if (normalizedDetail.includes('current password') || normalizedDetail.includes('incorrect')) {
      return 'The current password did not match this account. Re-enter the current password, then try again.'
    }
    if (normalizedDetail.includes('new password') || normalizedDetail.includes('password')) {
      return 'Choose a new password that meets the password rules, then try again.'
    }
    return 'Check the current password and make sure the new password meets the requirements, then try again.'
  }
  if (normalizedDetail.includes('already exists') || normalizedDetail.includes('taken')) {
    return 'That organization name is already in use. Choose a different display name, then try again.'
  }
  return 'Use an organization name between 1 and 100 characters, then try again.'
}

function shouldUseDetail(status: number | null): boolean {
  return status === null || status === 400 || status === 422
}

function statusFromAccountError(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const status = (error as { status?: unknown }).status
    if (typeof status === 'number') return status

    const statusCode = (error as { statusCode?: unknown }).statusCode
    if (typeof statusCode === 'number') return statusCode
  }

  const detail = rawDetailFromAccountError(error)
  const match = detail?.match(/\b(?:API|HTTP|Server error\s*\()? ?(\d{3})\b/i)
  return match ? Number(match[1]) : null
}

function rawDetailFromAccountError(error: unknown): string | null {
  if (typeof error === 'string' && error.trim()) return error.trim()
  if (error && typeof error === 'object') {
    const serverError = (error as { serverError?: unknown }).serverError
    if (typeof serverError === 'string' && serverError.trim()) return serverError.trim()

    const errorValue = (error as { error?: unknown }).error
    if (typeof errorValue === 'string' && errorValue.trim()) return errorValue.trim()

    const messageValue = (error as { message?: unknown }).message
    if (typeof messageValue === 'string' && messageValue.trim()) return messageValue.trim()
  }
  if (error instanceof Error && error.message.trim()) return error.message.trim()
  return null
}

function safeDetailFromAccountError(error: unknown): string | null {
  const rawDetail = rawDetailFromAccountError(error)
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
