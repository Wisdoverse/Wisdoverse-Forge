export type TaskDetailErrorAction =
  | 'approveTask'
  | 'loadAgents'
  | 'loadContext'
  | 'loadRuns'
  | 'previewContext'
  | 'publishTask'
  | 'retryTask'

const ACTION_FALLBACKS: Record<TaskDetailErrorAction, string> = {
  approveTask:
    'The task was not approved. Check that the task is still waiting for approval, then try again.',
  loadAgents: 'Available agents could not load. Refresh this task before assigning it.',
  loadContext: 'Task context could not load. Refresh the detail panel after the API is healthy.',
  loadRuns:
    'Run attempts could not load. Refresh Updates before deciding whether to retry this task.',
  previewContext: 'The context review could not load. Choose an available agent, then try again.',
  publishTask:
    'The task was not published with selected context. Review the context choices, then try again.',
  retryTask: 'The task was not retried. Refresh the task, then try Retry task again.',
}

export function taskDetailErrorMessage(action: TaskDetailErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (/no available agent|no agent.*available/.test(normalized)) {
    return 'No agent is available for this task. Start an agent or wait for one to finish, then try again.'
  }

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} The browser could not reach the server. Check your connection, then refresh the page.`
  }

  if (status === 401) {
    return 'Sign in again, then retry this task action. Code: 401.'
  }

  if (status === 403) {
    return 'You do not have permission to change this task. Ask an owner or admin to update your role. Code: 403.'
  }

  if (status === 404) {
    return 'This task was not found. Refresh the board, then open the task again. Code: 404.'
  }

  if (status === 409) {
    return 'This task changed while you were working. Refresh the detail panel, then try again. Code: 409.'
  }

  if (status === 422) {
    return 'This task action is missing required information. Check the task state and selected agent, then try again. Code: 422.'
  }

  if (status === 429) {
    return 'The server is busy with too many task requests. Wait a moment, then try again. Code: 429.'
  }

  if (status && status >= 500) {
    return 'The server had a problem while handling this task. Try again after the API is healthy. Code: 5xx.'
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
