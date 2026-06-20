export type ApprovalQueueErrorAction = 'approveCandidate' | 'loadQueue' | 'rejectCandidate'

const ACTION_FALLBACKS: Record<ApprovalQueueErrorAction, string> = {
  approveCandidate:
    'Check who can reuse it and the original task details, then save the item again. The item was not saved.',
  loadQueue:
    'Choose Load saved items again so you see the latest saved items. Saved items could not load.',
  rejectCandidate:
    'Choose Load saved items again, then choose Do not save again. The item stayed on the list.',
}

const ACTION_RETRY_STEPS: Record<ApprovalQueueErrorAction, string> = {
  approveCandidate: 'choose Save item again',
  loadQueue: 'choose Load saved items again',
  rejectCandidate: 'choose Do not save again',
}

export function approvalQueueErrorMessage(action: ApprovalQueueErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return networkRecoveryMessage(action)
  }

  if (status === 401) {
    return `Sign in again, then ${ACTION_RETRY_STEPS[action]}.`
  }

  if (status === 403) {
    return `Ask an owner or admin to let you save or skip saved notes and instructions, then ${ACTION_RETRY_STEPS[action]}. You do not have permission right now.`
  }

  if (status === 404) {
    return 'Choose Load saved items again so you see the latest saved items. This item was not found.'
  }

  if (status === 409) {
    return 'Choose Load saved items again, then open this item. It changed while you were checking it.'
  }

  if (status === 422) {
    return validationMessage(action, detail)
  }

  if (status === 429) {
    return `Wait a moment, then ${ACTION_RETRY_STEPS[action]}. Saved items are busy.`
  }

  if (status && status >= 500) {
    return serviceRecoveryMessage(action)
  }

  return validationMessage(action, detail)
}

function networkRecoveryMessage(action: ApprovalQueueErrorAction): string {
  if (action === 'loadQueue') {
    return 'Check your connection, then choose Load saved items again. Forge could not connect while loading saved notes and instructions.'
  }
  return `Check your connection, then ${ACTION_RETRY_STEPS[action]}. Forge could not connect while saving your choice.`
}

function serviceRecoveryMessage(action: ApprovalQueueErrorAction): string {
  if (action === 'loadQueue') {
    return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check Saved items access.`
  }
  return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check Saved items access.`
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
      ? 'Choose Load saved items again, then check who can reuse the selected items. Saved items could not load.'
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
