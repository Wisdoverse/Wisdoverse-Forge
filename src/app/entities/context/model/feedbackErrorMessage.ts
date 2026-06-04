const RAW_NETWORK_ERRORS = [/^Network error$/i, /^Failed to fetch$/i]
const RAW_STATUS_ERRORS = [/^API\s+\d{3}/i, /^HTTP\s+\d{3}/i, /^Server error\s*\(\d{3}\)$/i]
const GENERIC_BODY_TEXT = /^(Unauthorized|Forbidden|Not Found|Internal Server Error)$/i

export function feedbackErrorMessage(error?: unknown): string {
  const status = statusFromError(error)
  const detail = status === null || status === 400 || status === 422 ? safeDetail(error) : null
  const suffix = detail ? ` Details: ${detail}` : ''

  if (!status) {
    if (detail) {
      return `Feedback could not be saved. Review the message and try again.${suffix}`
    }
    return 'Feedback could not be saved because the browser could not reach the server. Check your connection, then try again.'
  }

  const statusText = `Code: ${status}.`
  if (status === 401) {
    return `Sign in again, then save this feedback. ${statusText}${suffix}`
  }
  if (status === 403) {
    return `You do not have permission to save feedback for this context. Ask an admin to check your role. ${statusText}${suffix}`
  }
  if (status === 404) {
    return `This context item could not be found. Refresh the task, then choose the context item again. ${statusText}${suffix}`
  }
  if (status === 409) {
    return `This context item changed while you were giving feedback. Refresh the task, review the item, then try again. ${statusText}${suffix}`
  }
  if (status === 400 || status === 422) {
    return `Choose one feedback option for this context item, then try again. ${statusText}${suffix}`
  }
  if (status === 429) {
    return `Feedback is busy. Wait a moment, then save this feedback again. ${statusText}${suffix}`
  }
  if (status >= 500) {
    return `The feedback service had a server problem. Try again after the backend is healthy. ${statusText}${suffix}`
  }

  return `Feedback could not be saved. Refresh the task and try again. ${statusText}${suffix}`
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
