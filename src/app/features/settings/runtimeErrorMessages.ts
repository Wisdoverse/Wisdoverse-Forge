export type RuntimeErrorAction = 'loadAgentSignals' | 'loadCliSignIn' | 'startCliSignIn'

const ACTION_FALLBACKS: Record<RuntimeErrorAction, string> = {
  loadAgentSignals:
    'Start or wake an agent, then refresh this page. Agent connection status could not load.',
  loadCliSignIn:
    'Refresh this page before starting agents that use work tools. Work tool sign-in could not be checked.',
  startCliSignIn:
    'Check the connected AI service, then reconnect the account. Work tool sign-in did not start.',
}

export function runtimeErrorMessage(action: RuntimeErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} Check your connection, then refresh Settings. Forge could not connect while checking Where agents work.`
  }

  if (status === 401) {
    return 'Sign in again, then open Where agents work and try again. Your sign-in expired.'
  }

  if (status === 403) {
    return 'Ask an owner or admin to update your team space access before changing Where agents work. You do not have permission to change Where agents work.'
  }

  if (status === 404) {
    return 'Refresh Settings. Where agents work is not available yet. If it still does not load, ask an owner or admin to check it.'
  }

  if (status === 409) {
    return 'Refresh this page, review the current status, then try again. The choices in Where agents work changed while you were working.'
  }

  if (status === 422) {
    return runtimeValidationMessage(action, detail)
  }

  if (status === 429) {
    return 'Wait a moment, then try again. Forge is receiving too many setup requests right now.'
  }

  if (status && status >= 500) {
    return 'Refresh this page, then try again. Forge could not check Where agents work right now. If it still fails, ask an owner or admin to check Where agents work in Settings.'
  }

  return runtimeValidationMessage(action, detail)
}

export function runtimeSettingsErrorMessage(err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)
  const isSaveAction =
    normalized.includes('update') ||
    normalized.includes('required fields for runtime setting') ||
    normalized.includes('default cli tool') ||
    normalized.includes('default runtime') ||
    normalized.includes('not available')
  const loadBase = 'Refresh Settings to load Where agents work.'

  if (isNetworkError(normalized)) {
    return isSaveAction
      ? 'Check your connection, then save Where agents work again. Forge could not connect while saving Where agents work.'
      : 'Check your connection, then refresh Settings to load Where agents work.'
  }

  if (status === 401) {
    return isSaveAction
      ? 'Sign in again, then save Where agents work again. Your sign-in expired.'
      : 'Sign in again, then open Where agents work. Your sign-in expired.'
  }

  if (status === 403) {
    return isSaveAction
      ? 'Ask an owner or admin for access to change Where agents work, then save again. Where agents work could not be saved.'
      : 'Ask an owner or admin for access to change Where agents work.'
  }

  if (status === 404) {
    return isSaveAction
      ? 'Refresh Settings, then save after Where agents work is available. Where agents work could not be saved.'
      : 'Refresh Settings after Where agents work is available.'
  }

  if (status === 409) {
    return isSaveAction
      ? 'Refresh Settings, review the current choices, then save again. The choices in Where agents work changed while you were working.'
      : 'Refresh Settings, review the current choices, then try again. The choices in Where agents work changed while you were working.'
  }

  if (status === 422) {
    return 'Choose where project files open and a work tool, then save again. Where agents work could not be saved.'
  }

  if (status === 429) {
    return isSaveAction
      ? 'Wait a minute, then save Where agents work again. Too many setup requests are happening right now.'
      : 'Wait a minute, then refresh Settings. Too many setup requests are happening right now.'
  }

  if (status && status >= 500) {
    return isSaveAction
      ? 'Refresh Settings, then save again. Where agents work could not be saved. If it still fails, ask an owner or admin to check Where agents work in Settings.'
      : `${loadBase} If it still fails, ask an owner or admin to check Where agents work in Settings.`
  }

  return isSaveAction
    ? 'Check where project files open and the work tool choice, then save Where agents work again. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    : `${loadBase} If it still fails, ask an owner or admin to check Where agents work in Settings.`
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

function runtimeValidationMessage(action: RuntimeErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()

  if (action === 'startCliSignIn') {
    if (normalized.includes('provider') || normalized.includes('configured')) {
      return 'Choose and save an AI service first, then reconnect the work tool sign-in.'
    }
    if (normalized.includes('tool') || normalized.includes('cli')) {
      return 'Choose an available work tool, then reconnect the work tool sign-in.'
    }
    return 'Check the connected AI service and selected work tool, then reconnect the work tool sign-in.'
  }

  if (action === 'loadCliSignIn') {
    return 'Refresh this page, then reconnect the work tool sign-in. Work tool sign-in could not be checked.'
  }

  return 'Start or wake an agent, then refresh this page. Agent connection status could not load.'
}
