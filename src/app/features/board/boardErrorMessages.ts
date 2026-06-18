export type BoardErrorAction =
  | 'createTask'
  | 'loadReadiness'
  | 'loadTasks'
  | 'moveTask'
  | 'previewContext'
  | 'publishTask'
  | 'selectProject'

const ACTION_FALLBACKS: Record<BoardErrorAction, string> = {
  createTask:
    'Check the project, where tasks wait, and the result, then create the task again. The task was not created.',
  loadReadiness: 'Refresh the board to load agent status before sending work.',
  loadTasks: 'Refresh the board to load tasks.',
  moveTask:
    'Refresh the board, then move the task again. The task was moved back because the board change was not saved.',
  previewContext: 'Choose an available agent, then open the saved notes review again.',
  publishTask:
    'Review the saved notes, then send the task with selected saved notes again. The task was not sent.',
  selectProject: 'Choose the project again, then create the task. The project was not selected.',
}

export function boardActionErrorMessage(action: BoardErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (/no available agent|no agent.*available/.test(normalized)) {
    return 'No agent can prepare the saved notes review right now. Open Agents to start or connect an agent, then return to the board and refresh.'
  }

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} ${networkRecoveryMessage(action)}`
  }

  if (status === 401) {
    return 'Sign in again, then open the board and try this action again.'
  }

  if (status === 403) {
    return 'Ask an owner or admin to give you access to this board, then refresh the board and try again. You do not have permission to change this board.'
  }

  if (status === 404) {
    return 'Refresh the board, then choose the current task again.'
  }

  if (status === 409) {
    return 'The board changed while you were working. Refresh the board so you see the latest tasks, then try again.'
  }

  if (status === 422) {
    return validationRecovery(action, detail)
  }

  if (status === 429) {
    return 'The board is busy with too many requests. Wait a moment, then try again.'
  }

  if (status && status >= 500) {
    return serviceRecoveryMessage(action)
  }

  return validationRecovery(action, detail)
}

function networkRecoveryMessage(action: BoardErrorAction): string {
  if (action === 'loadReadiness' || action === 'loadTasks') {
    return 'If it still does not load, check your connection and refresh the page.'
  }
  return 'If it still does not update, check your connection and try again.'
}

function serviceRecoveryMessage(action: BoardErrorAction): string {
  if (action === 'loadReadiness' || action === 'loadTasks') {
    return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check task board setup.`
  }
  return `${ACTION_FALLBACKS[action]} If it still fails, ask an owner or admin to check task board setup.`
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

function validationRecovery(action: BoardErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()

  if (normalized.includes('title') || normalized.includes('name')) {
    return 'Add a task result, choose the project and where tasks wait, then create the task again.'
  }
  if (normalized.includes('project')) {
    return 'Choose a project you can access, then try the board action again.'
  }
  if (normalized.includes('lane') || normalized.includes('group')) {
    return 'Choose where tasks wait for this project, then try the board action again.'
  }
  if (normalized.includes('agent')) {
    return 'Choose an available agent, then try the board action again.'
  }

  return ACTION_FALLBACKS[action]
}
