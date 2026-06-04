export type GovernanceAuditErrorAction = 'exportAudit' | 'loadAudit'

const ACTION_FALLBACKS: Record<GovernanceAuditErrorAction, string> = {
  exportAudit:
    'The audit export did not finish. Keep secrets hidden, refresh the audit view, then try export again.',
  loadAudit:
    'The governance audit could not load. Refresh after the API is healthy, then apply the filters again.',
}

export function governanceAuditErrorMessage(
  action: GovernanceAuditErrorAction,
  err: unknown
): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} The browser could not reach the server. Check your connection, then refresh the page.`
  }

  if (status === 401) {
    return 'Sign in again, then retry this audit action. Code: 401.'
  }

  if (status === 403) {
    return 'You do not have permission to view or export governance audit records. Ask an owner or admin to update your role. Code: 403.'
  }

  if (status === 404) {
    return 'The governance audit endpoint was not found. Refresh after the backend is deployed. Code: 404.'
  }

  if (status === 409) {
    return 'The audit data changed while export was running. Refresh the audit view, then export again. Code: 409.'
  }

  if (status === 422) {
    return 'The audit filters are not valid. Check the event name, IDs, time range, and record limit, then try again. Code: 422.'
  }

  if (status === 429) {
    return 'The server is busy with too many audit requests. Wait a moment, then try again. Code: 429.'
  }

  if (status && status >= 500) {
    return 'The server had a problem while handling governance audit records. Try again after the API is healthy. Code: 5xx.'
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
