export type GovernanceAuditErrorAction = 'exportAudit' | 'loadAudit'

const ACTION_FALLBACKS: Record<GovernanceAuditErrorAction, string> = {
  exportAudit:
    'The audit export did not finish. Keep secrets hidden, refresh the audit view, then try the export again.',
  loadAudit:
    'Governance audit history could not load. Refresh the audit view, then apply the filters again.',
}

export function governanceAuditErrorMessage(
  action: GovernanceAuditErrorAction,
  err: unknown
): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} Forge could not connect while ${actionLabel(action)}. Check your connection, then try again.`
  }

  if (status === 401) {
    return 'Your sign-in expired. Sign in again, then retry this audit action.'
  }

  if (status === 403) {
    return 'You do not have permission to view or export governance audit records. Ask an owner or admin to update your role.'
  }

  if (status === 404) {
    return 'Governance audit is not available from this view. Open the Admin audit view again, then retry. If it still fails, ask an owner or admin to check workspace access.'
  }

  if (status === 409) {
    return 'The audit data changed while export was running. Refresh the audit view, then export again.'
  }

  if (status === 422) {
    return validationMessage(action, detail)
  }

  if (status === 429) {
    return 'Governance audit is handling too many requests right now. Wait a moment, then try again.'
  }

  if (status && status >= 500) {
    return `Forge could not ${actionVerb(action)} governance audit history right now. Refresh the audit view, then try again. If it still fails, ask an owner or admin to check governance audit setup.`
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

function actionLabel(action: GovernanceAuditErrorAction): string {
  return action === 'exportAudit' ? 'exporting audit history' : 'loading audit history'
}

function actionVerb(action: GovernanceAuditErrorAction): string {
  return action === 'exportAudit' ? 'export' : 'load'
}

function validationMessage(action: GovernanceAuditErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()
  if (normalized.includes('time')) {
    return 'Choose a valid time range. Make sure From is before To, then apply the audit filters again.'
  }
  if (normalized.includes('limit')) {
    return 'Enter a record limit from 1 to 200, then apply the audit filters again.'
  }
  if (normalized.includes('event')) {
    return 'Choose a common audit view or paste a support event name, then apply the audit filters again.'
  }
  if (normalized.includes('id')) {
    return 'Check the selected organization, workspace, user, or task support reference, then apply the audit filters again.'
  }
  return ACTION_FALLBACKS[action]
}
