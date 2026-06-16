export type TaskDetailErrorAction =
  | 'approveTask'
  | 'blockTask'
  | 'cancelTask'
  | 'loadAgents'
  | 'loadContext'
  | 'loadRuns'
  | 'previewContext'
  | 'publishTask'
  | 'retryTask'

const ACTION_FALLBACKS: Record<TaskDetailErrorAction, string> = {
  approveTask:
    'The task was not approved. Check that the task is still waiting for approval, then try again.',
  blockTask:
    'The task was not marked as needing help. Refresh the task, then choose Needs help again.',
  cancelTask: 'The task was not canceled. Refresh the task, then choose Cancel again.',
  loadAgents: 'Refresh this task before assigning an agent.',
  loadContext: 'Refresh the detail panel to load saved notes and work history.',
  loadRuns: 'Refresh Updates before deciding whether to retry this task.',
  previewContext: 'Choose an available agent, then open saved item review again.',
  publishTask: 'The task was not sent with selected notes. Review the saved notes, then try again.',
  retryTask: 'The task was not retried. Refresh the task, then try Retry task again.',
}

export function taskDetailErrorMessage(action: TaskDetailErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (/no available agent|no agent.*available/.test(normalized)) {
    return 'No agent can take this task right now. Open Agents to start or connect an agent, then refresh this task and try again.'
  }

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} ${networkRecoveryMessage(action)}`
  }

  if (status === 401) {
    return 'Sign in again, then retry this task action.'
  }

  if (status === 403) {
    if (action === 'loadAgents' || action === 'loadContext' || action === 'loadRuns') {
      return 'Ask an owner or admin to give you access to this task, then refresh the task detail panel. You do not have permission to view this task.'
    }
    return 'Ask an owner or admin to let you update this task, then refresh the task detail panel and try again. You do not have permission to change this task.'
  }

  if (status === 404) {
    return 'This task was not found. Refresh the board, then open the task again.'
  }

  if (status === 409) {
    return 'This task changed while you were working. Refresh the detail panel, then try again.'
  }

  if (status === 422) {
    return validationMessage(action, detail)
  }

  if (status === 429) {
    return 'Task actions are busy. Wait a moment, then try again.'
  }

  if (status && status >= 500) {
    return serviceRecoveryMessage(action)
  }

  return validationMessage(action, detail)
}

function networkRecoveryMessage(action: TaskDetailErrorAction): string {
  if (action === 'loadAgents' || action === 'loadContext' || action === 'loadRuns') {
    return 'If it still does not load, check your connection and refresh the page.'
  }
  return 'If it still does not update, check your connection and try again.'
}

function serviceRecoveryMessage(action: TaskDetailErrorAction): string {
  if (action === 'loadAgents' || action === 'loadContext' || action === 'loadRuns') {
    return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check task setup.`
  }
  return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check task setup.`
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

function validationMessage(action: TaskDetailErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()
  if (normalized.includes('already running')) {
    return 'This task is already in progress. Wait for the current work to finish, then refresh the task.'
  }
  if (normalized.includes('agent')) {
    return 'Choose an available agent, then try again.'
  }
  if (normalized.includes('context')) {
    return 'Review the selected saved notes, then try again.'
  }
  if (normalized.includes('approval') || normalized.includes('approve')) {
    return 'Check that the task is still waiting for approval, then try again.'
  }
  if (normalized.includes('publish')) {
    return 'Review the task details, then send again.'
  }
  return ACTION_FALLBACKS[action]
}
