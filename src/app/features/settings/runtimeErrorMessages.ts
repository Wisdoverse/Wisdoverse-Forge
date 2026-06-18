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
    return `${ACTION_FALLBACKS[action]} Check your connection, then refresh Settings. Forge could not connect while checking Agent work setup.`
  }

  if (status === 401) {
    return 'Sign in again, then open Agent work setup and try again. Your sign-in expired.'
  }

  if (status === 403) {
    return 'Ask an owner or admin to update your team space access before changing Agent work setup. You do not have permission to change Agent work setup.'
  }

  if (status === 404) {
    return 'Refresh Settings. Agent work setup is not available yet. If it still does not load, ask an owner or admin to check it.'
  }

  if (status === 409) {
    return 'Refresh this page, review the current status, then try again. The Agent work setup choices changed while you were working.'
  }

  if (status === 422) {
    return runtimeValidationMessage(action, detail)
  }

  if (status === 429) {
    return 'Wait a moment, then try again. Forge is receiving too many setup requests right now.'
  }

  if (status && status >= 500) {
    return 'Refresh this page, then try again. Forge could not check Agent work setup right now. If it still fails, ask an owner or admin to check Agent work setup in Settings.'
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
  const loadBase = 'Refresh Settings to load Agent work setup.'

  if (isNetworkError(normalized)) {
    return isSaveAction
      ? 'Check your connection, then save Agent work setup again. Forge could not connect while saving Agent work setup.'
      : 'Check your connection, then refresh Settings to load Agent work setup.'
  }

  if (status === 401) {
    return isSaveAction
      ? 'Sign in again, then save Agent work setup again. Your sign-in expired.'
      : 'Sign in again, then open Agent work setup. Your sign-in expired.'
  }

  if (status === 403) {
    return isSaveAction
      ? 'Ask an owner or admin for access to change Agent work setup, then save again. Agent work setup could not be saved.'
      : 'Ask an owner or admin for access to change Agent work setup.'
  }

  if (status === 404) {
    return isSaveAction
      ? 'Refresh Settings, then save after Agent work setup is available. Agent work setup could not be saved.'
      : 'Refresh Settings after Agent work setup is available.'
  }

  if (status === 409) {
    return isSaveAction
      ? 'Refresh Settings, review the current choices, then save again. The Agent work setup choices changed while you were working.'
      : 'Refresh Settings, review the current choices, then try again. The Agent work setup choices changed while you were working.'
  }

  if (status === 422) {
    return 'Choose an available agent location and work tool, then save again. Agent work setup could not be saved.'
  }

  if (status === 429) {
    return isSaveAction
      ? 'Wait a minute, then save Agent work setup again. Too many setup requests are happening right now.'
      : 'Wait a minute, then refresh Settings. Too many setup requests are happening right now.'
  }

  if (status && status >= 500) {
    return isSaveAction
      ? 'Refresh Settings, then save again. Agent work setup could not be saved. If it still fails, ask an owner or admin to check Agent work setup in Settings.'
      : `${loadBase} If it still fails, ask an owner or admin to check Agent work setup in Settings.`
  }

  return isSaveAction
    ? 'Check the agent location and work tool choices, then save Agent work setup again. If it still fails, ask an owner or admin to check Agent work setup in Settings.'
    : `${loadBase} If it still fails, ask an owner or admin to check Agent work setup in Settings.`
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
