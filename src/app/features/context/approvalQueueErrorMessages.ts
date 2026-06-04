export type ApprovalQueueErrorAction = 'approveCandidate' | 'loadQueue' | 'rejectCandidate'

const ACTION_FALLBACKS: Record<ApprovalQueueErrorAction, string> = {
  approveCandidate:
    'The candidate was not approved. Review the scope and source preview, then try again.',
  loadQueue:
    'The approval queue could not load. Refresh after the API is healthy so you see the latest candidates.',
  rejectCandidate:
    'The candidate was not rejected. Refresh the queue, then try the reject action again.',
}

export function approvalQueueErrorMessage(action: ApprovalQueueErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} The browser could not reach the server. Check your connection, then refresh the page.`
  }

  if (status === 401) {
    return 'Sign in again, then retry this approval queue action. Code: 401.'
  }

  if (status === 403) {
    return 'You do not have permission to manage reusable context. Ask an owner or admin to update your role. Code: 403.'
  }

  if (status === 404) {
    return 'This candidate was not found. Refresh the queue so you see the latest candidates. Code: 404.'
  }

  if (status === 409) {
    return 'This candidate changed while you were reviewing it. Refresh the queue, then open it again. Code: 409.'
  }

  if (status === 422) {
    return 'This approval request is missing required information. Check the scope, sensitivity, and confirmation, then try again. Code: 422.'
  }

  if (status === 429) {
    return 'The server is busy with too many approval requests. Wait a moment, then try again. Code: 429.'
  }

  if (status && status >= 500) {
    return 'The server had a problem while handling the approval queue. Try again after the API is healthy. Code: 5xx.'
  }

  const suffix = operatorSafeDetail(detail)
  return suffix ? `${ACTION_FALLBACKS[action]} Detail: ${suffix}` : ACTION_FALLBACKS[action]
}

function errorDetail(err: unknown): string {
  if (err instanceof Error) return err.message
  if (typeof err === 'string') return err
  if (!err || typeof err !== 'object') return ''

  const value = err as {
    detail?: unknown
    error?: unknown
    message?: unknown
    reason?: unknown
  }

  for (const candidate of [value.detail, value.error, value.message, value.reason]) {
    if (typeof candidate === 'string' && candidate.trim()) return candidate.trim()
  }

  return ''
}

function errorStatus(err: unknown, normalizedDetail: string): number | null {
  if (err && typeof err === 'object') {
    const value = err as { status?: unknown; statusCode?: unknown; code?: unknown }
    for (const candidate of [value.status, value.statusCode, value.code]) {
      const status = numericStatus(candidate)
      if (status) return status
    }
  }

  const match = normalizedDetail.match(/\b(401|403|404|409|422|429|5\d{2})\b/)
  return match ? Number(match[1]) : null
}

function numericStatus(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && /^\d+$/.test(value)) return Number(value)
  return null
}

function isNetworkError(normalizedDetail: string): boolean {
  return (
    normalizedDetail === 'network error' ||
    normalizedDetail === 'failed to fetch' ||
    normalizedDetail === 'load failed' ||
    normalizedDetail.includes('networkerror') ||
    normalizedDetail.includes('connection refused') ||
    normalizedDetail.includes('could not reach')
  )
}

function operatorSafeDetail(detail: string): string {
  const trimmed = detail.trim()
  if (!trimmed) return ''
  if (trimmed.length > 160) return ''
  if (isNetworkError(trimmed.toLowerCase())) return ''
  return trimmed
}
