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
  loadTasks: 'The task board could not load. Refresh the board after the API is healthy.',
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
    return 'Sign in again, then retry this board action. Code: 401.'
  }

  if (status === 403) {
    return 'You do not have permission to change this board. Ask an owner or admin to update your role. Code: 403.'
  }

  if (status === 404) {
    return 'This board item was not found. Refresh the board, then try the action again. Code: 404.'
  }

  if (status === 409) {
    return 'The board changed while you were working. Refresh the board so you see the latest tasks, then try again. Code: 409.'
  }

  if (status === 422) {
    return 'The board request is missing required task information. Check the project, work lane, and task title, then try again. Code: 422.'
  }

  if (status === 429) {
    return 'The server is busy with too many board requests. Wait a moment, then try again. Code: 429.'
  }

  if (status && status >= 500) {
    return 'The server had a problem while handling the board. Try again after the API is healthy. Code: 5xx.'
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
