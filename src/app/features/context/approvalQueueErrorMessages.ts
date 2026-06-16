export type ApprovalQueueErrorAction = 'approveCandidate' | 'loadQueue' | 'rejectCandidate'

const ACTION_FALLBACKS: Record<ApprovalQueueErrorAction, string> = {
  approveCandidate:
    'Check who can reuse it and the original task preview, then approve the item again. The item was not approved.',
  loadQueue:
    'Refresh the list so you see the latest saved items. The saved item review list could not load.',
  rejectCandidate: 'Refresh the list, then reject the item again. The item was not rejected.',
}

export function approvalQueueErrorMessage(action: ApprovalQueueErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return networkRecoveryMessage(action)
  }

  if (status === 401) {
    return 'Sign in again, then retry this review action.'
  }

  if (status === 403) {
    return 'Ask an owner or admin to let you approve saved notes and instructions, then retry this review action. You do not have permission right now.'
  }

  if (status === 404) {
    return 'Refresh the list so you see the latest saved items. This item was not found.'
  }

  if (status === 409) {
    return 'Refresh the list, then open this item again. It changed while you were reviewing it.'
  }

  if (status === 422) {
    return validationMessage(action, detail)
  }

  if (status === 429) {
    return 'Wait a moment, then try again. The saved item review list is busy.'
  }

  if (status && status >= 500) {
    return serviceRecoveryMessage(action)
  }

  return validationMessage(action, detail)
}

function networkRecoveryMessage(action: ApprovalQueueErrorAction): string {
  if (action === 'loadQueue') {
    return 'Check your connection, then refresh the saved item review list. Forge could not connect while loading saved items.'
  }
  return 'Check your connection, then try this review action again. Forge could not connect while saving this review decision.'
}

function serviceRecoveryMessage(action: ApprovalQueueErrorAction): string {
  if (action === 'loadQueue') {
    return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check saved item setup.`
  }
  return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check saved item setup.`
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

function validationMessage(action: ApprovalQueueErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()
  if (normalized.includes('scope')) {
    return action === 'loadQueue'
      ? 'Refresh the list, then check who can reuse the selected items. The saved item review list could not load.'
      : 'Choose who can reuse it and review the original task preview, then try again.'
  }
  if (normalized.includes('sensitivity')) {
    return 'Choose the sensitivity level, then try again.'
  }
  if (normalized.includes('confirmation') || normalized.includes('confirm')) {
    return 'Complete the confirmation step, then try again.'
  }
  return ACTION_FALLBACKS[action]
}
