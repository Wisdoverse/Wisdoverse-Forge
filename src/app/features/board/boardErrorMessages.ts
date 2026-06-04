export type BoardErrorAction =
  | 'createTask'
  | 'loadReadiness'
  | 'loadTasks'
  | 'moveTask'
  | 'previewContext'
  | 'publishTask'
  | 'selectProject'

const ACTION_FALLBACKS: Record<BoardErrorAction, string> = {
  createTask: 'The task was not created. Check the project, work lane, and title, then try again.',
  loadReadiness:
    'Agent readiness could not load. Refresh readiness before assigning or publishing work.',
  loadTasks: 'The task board could not load. Refresh the board, then try again.',
  moveTask: 'The task was moved back because the server did not save the board change.',
  previewContext: 'The context preview could not load. Choose an available agent, then try again.',
  publishTask: 'The task was not published with context. Review the preview, then try again.',
  selectProject: 'The project was not selected. Choose the project again, then create the task.',
}

export function boardActionErrorMessage(action: BoardErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (/no available agent|no agent.*available/.test(normalized)) {
    return 'No agent is available for context preview. Start an agent or wait for one to finish, then try again.'
  }

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} The browser could not reach the server. Check your connection, then refresh the page.`
  }

  if (status === 401) {
    return 'Sign in again, then open the board and try this action again.'
  }

  if (status === 403) {
    return 'You do not have permission to change this board. Ask an owner or admin to update your role.'
  }

  if (status === 404) {
    return 'This board item was not found. Refresh the board, then choose the current task again.'
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
    return 'The board is temporarily unavailable. Refresh the board, then try again. If it still fails, ask an owner or admin to check task board setup.'
  }

  return validationRecovery(action, detail)
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

function validationRecovery(action: BoardErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()

  if (normalized.includes('title') || normalized.includes('name')) {
    return 'Add a task title, choose the project and work lane, then create the task again.'
  }
  if (normalized.includes('project')) {
    return 'Choose a project you can access, then try the board action again.'
  }
  if (normalized.includes('lane') || normalized.includes('group')) {
    return 'Choose a work lane for this project, then try the board action again.'
  }
  if (normalized.includes('agent')) {
    return 'Choose an available agent, then try the board action again.'
  }

  return ACTION_FALLBACKS[action]
}
