export type GovernanceAuditErrorAction = 'exportAudit' | 'loadAudit'

const ACTION_FALLBACKS: Record<GovernanceAuditErrorAction, string> = {
  exportAudit: 'Keep secrets hidden, choose Refresh change history, then export again.',
  loadAudit: 'Choose Refresh change history, then apply the filters again.',
}

const ACTION_RETRY_STEPS: Record<GovernanceAuditErrorAction, string> = {
  exportAudit: 'choose Export change history again',
  loadAudit: 'choose Refresh change history again',
}

export function governanceAuditErrorMessage(
  action: GovernanceAuditErrorAction,
  err: unknown
): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} ${networkRecoveryMessage(action)}`
  }

  if (status === 401) {
    return `Your sign-in expired. Sign in again, then ${ACTION_RETRY_STEPS[action]}.`
  }

  if (status === 403) {
    return `Ask an owner or admin to update your team space access, then ${ACTION_RETRY_STEPS[action]}. You do not have permission to view or export change history.`
  }

  if (status === 404) {
    return `Open Admin change history again, then ${ACTION_RETRY_STEPS[action]}. If it still fails, ask an owner or admin to check team space access.`
  }

  if (status === 409) {
    return action === 'exportAudit'
      ? 'Choose Refresh change history, then choose Export change history again because the change list changed while export was running.'
      : 'Choose Refresh change history again because the change list changed while you were checking it.'
  }

  if (status === 422) {
    return validationMessage(action, detail)
  }

  if (status === 429) {
    return `Wait a moment, then ${ACTION_RETRY_STEPS[action]}. Change history is handling too many requests right now.`
  }

  if (status && status >= 500) {
    return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check change history access.`
  }

  return validationMessage(action, detail)
}

function errorDetail(err: unknown): string {
  if (err instanceof Error) return err.message
  if (typeof err === 'string') return err
  if (!err || typeof err !== 'object') return ''

  const value = err as {
    serverError?: unknown
    detail?: unknown
    error?: unknown
    message?: unknown
    reason?: unknown
  }

  for (const candidate of [
    value.serverError,
    value.detail,
    value.error,
    value.message,
    value.reason,
  ]) {
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

function networkRecoveryMessage(action: GovernanceAuditErrorAction): string {
  if (action === 'exportAudit') {
    return 'If it still does not export, check your connection and choose Export change history again.'
  }
  return 'If it still does not load, check your connection and choose Refresh change history again.'
}

function validationMessage(action: GovernanceAuditErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()
  if (normalized.includes('time')) {
    return 'Choose a valid time range. Make sure From is before To, then apply the change filters again.'
  }
  if (normalized.includes('limit')) {
    return 'Enter a row limit from 1 to 200, then apply the change filters again.'
  }
  if (normalized.includes('event')) {
    return 'Choose a common change view or paste a specific change name, then apply the change filters again.'
  }
  if (normalized.includes('id')) {
    return 'Check the selected team space, work area, person, or task code, then apply the change filters again.'
  }
  return ACTION_FALLBACKS[action]
}
