const RAW_NETWORK_ERRORS = [/^Network error$/i, /^Failed to fetch$/i]
const RAW_STATUS_ERRORS = [/^API\s+\d{3}/i, /^HTTP\s+\d{3}/i, /^Server error\s*\(\d{3}\)$/i]
const GENERIC_BODY_TEXT = /^(Unauthorized|Forbidden|Not Found|Internal Server Error)$/i

export function feedbackErrorMessage(error?: unknown): string {
  const status = statusFromError(error)
  const detail = status === null || status === 400 || status === 422 ? safeDetail(error) : null

  if (!status) {
    if (detail) {
      return validationMessage(detail)
    }
    return 'Feedback could not be saved. Forge could not connect while saving it. Check your connection, then try again.'
  }

  if (status === 401) {
    return 'Sign in again, then save this feedback.'
  }
  if (status === 403) {
    return 'You do not have permission to save feedback for this saved item. Ask an owner or admin to check your role.'
  }
  if (status === 404) {
    return 'This saved item could not be found. Refresh the task, then choose it again.'
  }
  if (status === 409) {
    return 'This saved item changed while you were giving feedback. Refresh the task, review it, then try again.'
  }
  if (status === 400 || status === 422) {
    return validationMessage(detail)
  }
  if (status === 429) {
    return 'Feedback is busy. Wait a moment, then save this feedback again.'
  }
  if (status >= 500) {
    return 'Forge could not save feedback right now. Refresh the task, then try again. If it still fails, ask an owner or admin to check feedback setup.'
  }

  return 'Feedback could not be saved. Refresh the task and try again.'
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

function validationMessage(detail: string | null): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (
    normalized.includes('option') ||
    normalized.includes('vote') ||
    normalized.includes('rating')
  ) {
    return 'Choose one feedback option for this saved item, then try again.'
  }
  if (normalized.includes('context')) {
    return 'Refresh the task, choose the saved item again, then save feedback.'
  }
  return 'Choose one feedback option for this saved item, then try again.'
}
