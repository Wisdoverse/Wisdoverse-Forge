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
    'Check that the task is still waiting for your decision, then choose Allow and continue again. The task did not continue.',
  blockTask:
    'Open this task again from the Tasks page, then choose Needs help again. The task was not marked as needing help.',
  cancelTask:
    'Open this task again from the Tasks page, then choose Cancel again. The task was not canceled.',
  loadAgents: 'Open this task again from the Tasks page before assigning an agent.',
  loadContext: 'Open this task again from the Tasks page to load saved notes and work history.',
  loadRuns: 'Open Updates for this task again before deciding whether to retry this task.',
  previewContext: 'Choose an available agent, then check saved items again.',
  publishTask:
    'Check the selected saved notes, then send the task again. The task was not sent with selected notes.',
  retryTask:
    'Open this task again from the Tasks page, then choose Retry task again. The task was not retried.',
}

export function taskDetailErrorMessage(action: TaskDetailErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (/no available agent|no agent.*available/.test(normalized)) {
    return 'No agent can take this task right now. Open Agents to start or connect an agent, then open this task again from the Tasks page.'
  }

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} ${networkRecoveryMessage(action)}`
  }

  if (status === 401) {
    return 'Sign in again, then retry this task action.'
  }

  if (status === 403) {
    if (action === 'loadAgents' || action === 'loadContext' || action === 'loadRuns') {
      return 'Ask an owner or admin to give you access to this task, then open it again from the Tasks page. You do not have permission to view this task.'
    }
    return 'Ask an owner or admin to let you update this task, then open it again from the Tasks page and try again. You do not have permission to change this task.'
  }

  if (status === 404) {
    return 'Open the Tasks page, then choose the current task again. This task was not found.'
  }

  if (status === 409) {
    return 'Open this task again from the Tasks page, then try again. This task changed while you were working.'
  }

  if (status === 422) {
    return validationMessage(action, detail)
  }

  if (status === 429) {
    return busyTaskActionMessage(action)
  }

  if (status && status >= 500) {
    return serviceRecoveryMessage(action)
  }

  return validationMessage(action, detail)
}

function busyTaskActionMessage(action: TaskDetailErrorAction): string {
  if (action === 'loadAgents' || action === 'loadContext' || action === 'loadRuns') {
    return 'Wait a moment, then open this task again from the Tasks page. Task details are busy right now.'
  }
  return `${ACTION_FALLBACKS[action]} Wait a moment before choosing the action again. Task actions are busy right now.`
}

function networkRecoveryMessage(action: TaskDetailErrorAction): string {
  if (action === 'loadAgents' || action === 'loadContext' || action === 'loadRuns') {
    return 'If it still does not load, check your connection, then open this task again from the Tasks page.'
  }
  return 'If it still does not update, check your connection, open this task again from the Tasks page, then choose the action again.'
}

function serviceRecoveryMessage(action: TaskDetailErrorAction): string {
  if (action === 'loadAgents' || action === 'loadContext' || action === 'loadRuns') {
    return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check task details access.`
  }
  return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check task action access.`
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
    return 'This task is already in progress. Wait for the current work to finish, then open this task again from the Tasks page.'
  }
  if (normalized.includes('agent')) {
    return 'Choose an available agent, then open this task again from the Tasks page and choose the action again.'
  }
  if (normalized.includes('context')) {
    return 'Check the selected saved notes, then open this task again from the Tasks page and choose the action again.'
  }
  if (normalized.includes('approval') || normalized.includes('approve')) {
    return 'Check that the task is still waiting for your decision, then choose Allow and continue again.'
  }
  if (normalized.includes('publish')) {
    return 'Check the task details, then send the task again.'
  }
  return ACTION_FALLBACKS[action]
}
