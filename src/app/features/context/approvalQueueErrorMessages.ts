export type ApprovalQueueErrorAction = 'approveCandidate' | 'loadQueue' | 'rejectCandidate'

const ACTION_FALLBACKS: Record<ApprovalQueueErrorAction, string> = {
  approveCandidate:
    'Check who can reuse it and the original task details, then save the item again. The item was not saved.',
  loadQueue:
    'Choose Check Context again so you see the latest context items. Context items could not load.',
  rejectCandidate:
    'Choose Check Context again, then choose Do not save again. The item stayed on the list.',
}

const ACTION_RETRY_STEPS: Record<ApprovalQueueErrorAction, string> = {
  approveCandidate: 'choose Save item again',
  loadQueue: 'choose Check Context again',
  rejectCandidate: 'choose Do not save again',
}

export function approvalQueueErrorMessage(action: ApprovalQueueErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return networkRecoveryMessage(action)
  }

  if (status == null && isServiceError(detail)) {
    return serviceRecoveryMessage(action)
  }

  if (status === 401) {
    return `Sign in again, then ${ACTION_RETRY_STEPS[action]}.`
  }

  if (status === 403) {
    return `Ask an owner or admin to let you save or skip saved notes and guidance, then ${ACTION_RETRY_STEPS[action]}. You do not have permission right now.`
  }

  if (status === 404) {
    return 'Choose Check Context again so you see the latest context items. This item was not found.'
  }

  if (status === 409) {
    return 'Choose Check Context again, then open this item. It changed while you were checking it.'
  }

  if (status === 422) {
    return validationMessage(action, detail)
  }

  if (status === 429) {
    return `Wait a moment, then ${ACTION_RETRY_STEPS[action]}. Context items are busy.`
  }

  if (status && status >= 500) {
    return serviceRecoveryMessage(action)
  }

  return validationMessage(action, detail)
}

function networkRecoveryMessage(action: ApprovalQueueErrorAction): string {
  if (action === 'loadQueue') {
    return 'Check your connection, then choose Check Context again. Forge could not connect while loading saved notes and guidance.'
  }
  return `Check your connection, then ${ACTION_RETRY_STEPS[action]}. Forge could not connect while saving your choice.`
}

function serviceRecoveryMessage(action: ApprovalQueueErrorAction): string {
  if (action === 'loadQueue') {
    return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check Context access.`
  }
  if (action === 'approveCandidate') {
    return 'Wait a few minutes, then choose Save item again. The item was not saved. If it still fails, ask an owner or admin to check Context access.'
  }
  return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check Context access.`
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
    const text = payloadText(candidate)
    if (text) return text
  }

  return ''
}

function payloadText(value: unknown): string | null {
  if (typeof value === 'string' && value.trim()) return value.trim()
  if (!value || typeof value !== 'object') return null

  const record = value as Record<string, unknown>
  for (const key of ['serverError', 'message', 'error', 'detail', 'reason']) {
    const text = payloadText(record[key])
    if (text) return text
  }

  return null
}

function errorStatus(err: unknown, normalizedDetail: string): number | null {
  if (err && typeof err === 'object') {
    const value = err as { status?: unknown; statusCode?: unknown; code?: unknown }
    for (const candidate of [value.status, value.statusCode, value.code]) {
      const status = numericStatus(candidate)
      if (status) return status
    }
  }

  if (normalizedDetail.includes('role required')) return 403
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

function isServiceError(detail: string): boolean {
  return /\b(database|sql|stack trace|traceback|exception|panic|internal server error)\b/i.test(
    detail
  )
}

function validationMessage(action: ApprovalQueueErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()
  if (normalized.includes('scope')) {
    return action === 'loadQueue'
      ? 'Choose Check Context again, then check who can reuse the selected items. Context items could not load.'
      : `Choose who can reuse it and check the original task details, then ${ACTION_RETRY_STEPS[action]}.`
  }
  if (normalized.includes('sensitivity')) {
    return `Choose the sensitivity level, then ${ACTION_RETRY_STEPS[action]}.`
  }
  if (normalized.includes('confirmation') || normalized.includes('confirm')) {
    return `Complete the confirmation step, then ${ACTION_RETRY_STEPS[action]}.`
  }
  return ACTION_FALLBACKS[action]
}
