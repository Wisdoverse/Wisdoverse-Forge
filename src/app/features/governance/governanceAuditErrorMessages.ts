export type GovernanceAuditErrorAction = 'exportAudit' | 'loadAudit'

const ACTION_FALLBACKS: Record<GovernanceAuditErrorAction, string> = {
  exportAudit:
    'The audit export did not finish. Keep secrets hidden, refresh the audit view, then try export again.',
  loadAudit:
    'The governance audit could not load. Refresh the audit view, then apply the filters again.',
}

export function governanceAuditErrorMessage(
  action: GovernanceAuditErrorAction,
  err: unknown
): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} The app could not reach the service. Check your connection, then refresh the page.`
  }

  if (status === 401) {
    return 'Sign in again, then retry this audit action.'
  }

  if (status === 403) {
    return 'You do not have permission to view or export governance audit records. Ask an owner or admin to update your role.'
  }

  if (status === 404) {
    return 'Governance audit is not available from this page. Refresh the audit view, then try again.'
  }

  if (status === 409) {
    return 'The audit data changed while export was running. Refresh the audit view, then export again.'
  }

  if (status === 422) {
    return validationMessage(action, detail)
  }

  if (status === 429) {
    return 'Governance audit is busy. Wait a moment, then try again.'
  }

  if (status && status >= 500) {
    return 'Governance audit is temporarily unavailable. Refresh the audit view, then try again. If it still fails, ask an owner or admin to check governance audit setup.'
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

function validationMessage(action: GovernanceAuditErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()
  if (normalized.includes('time')) {
    return 'Choose a valid time range, then apply the audit filters again.'
  }
  if (normalized.includes('limit')) {
    return 'Choose a smaller record limit, then apply the audit filters again.'
  }
  if (normalized.includes('event')) {
    return 'Check the event name filter, then apply the audit filters again.'
  }
  if (normalized.includes('id')) {
    return 'Check the selected organization, workspace, user, or task ID, then apply the audit filters again.'
  }
  return ACTION_FALLBACKS[action]
}
