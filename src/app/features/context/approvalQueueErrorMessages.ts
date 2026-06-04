export type ApprovalQueueErrorAction = 'approveCandidate' | 'loadQueue' | 'rejectCandidate'

const ACTION_FALLBACKS: Record<ApprovalQueueErrorAction, string> = {
  approveCandidate:
    'The candidate was not approved. Review the scope and source preview, then try again.',
  loadQueue:
    'The approval queue could not load. Refresh the queue so you see the latest candidates.',
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
    return 'Sign in again, then retry this approval queue action.'
  }

  if (status === 403) {
    return 'You do not have permission to manage reusable context. Ask an owner or admin to update your role.'
  }

  if (status === 404) {
    return 'This candidate was not found. Refresh the queue so you see the latest candidates.'
  }

  if (status === 409) {
    return 'This candidate changed while you were reviewing it. Refresh the queue, then open it again.'
  }

  if (status === 422) {
    return validationMessage(action, detail)
  }

  if (status === 429) {
    return 'The approval queue is busy. Wait a moment, then try again.'
  }

  if (status && status >= 500) {
    return 'The approval queue is temporarily unavailable. Refresh the queue, then try again. If it still fails, ask an owner or admin to check reusable context setup.'
  }

  return validationMessage(action, detail)
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

function validationMessage(action: ApprovalQueueErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()
  if (normalized.includes('scope')) {
    return action === 'loadQueue'
      ? 'The approval queue could not load. Refresh the queue, then check the selected scope.'
      : 'Choose the scope and review the source preview, then try again.'
  }
  if (normalized.includes('sensitivity')) {
    return 'Choose the sensitivity level, then try again.'
  }
  if (normalized.includes('confirmation') || normalized.includes('confirm')) {
    return 'Complete the confirmation step, then try again.'
  }
  return ACTION_FALLBACKS[action]
}
