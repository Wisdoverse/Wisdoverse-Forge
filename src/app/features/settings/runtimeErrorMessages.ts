export type RuntimeErrorAction = 'loadAgentSignals' | 'loadCliSignIn' | 'startCliSignIn'

const ACTION_FALLBACKS: Record<RuntimeErrorAction, string> = {
  loadAgentSignals:
    'Agent connection status could not load. Start or wake an agent, then refresh this page.',
  loadCliSignIn:
    'Work tool sign-in could not be checked. Refresh this page before starting agents that use work tools.',
  startCliSignIn:
    'Work tool sign-in did not start. Check the connected AI service, then reconnect the account.',
}

export function runtimeErrorMessage(action: RuntimeErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} Forge could not connect while checking where agents run. Check your connection, then refresh Settings.`
  }

  if (status === 401) {
    return 'Your sign-in expired. Sign in again, then open Where agents run and try again.'
  }

  if (status === 403) {
    return 'You do not have permission to change where agents run. Ask an owner or admin to update your role.'
  }

  if (status === 404) {
    return 'Where agents run is not available yet. Refresh Settings. If it still does not load, ask an owner or admin to check it.'
  }

  if (status === 409) {
    return 'The Where agents run choices changed while you were working. Refresh this page, review the current status, then try again.'
  }

  if (status === 422) {
    return runtimeValidationMessage(action, detail)
  }

  if (status === 429) {
    return 'Forge is receiving too many setup requests right now. Wait a moment, then try again.'
  }

  if (status && status >= 500) {
    return 'Forge could not check where agents run right now. Refresh this page, then try again. If it still fails, ask an owner or admin to check Where agents run.'
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
  const saveBase = 'Where agents run could not be saved.'
  const loadBase = 'Refresh Settings to load Where agents run.'

  if (isNetworkError(normalized)) {
    return isSaveAction
      ? `${saveBase} Forge could not connect while saving where agents run. Check your connection, then save again.`
      : 'Check your connection, then refresh Settings to load Where agents run.'
  }

  if (status === 401) {
    return isSaveAction
      ? `${saveBase} Your sign-in expired. Sign in again, then save Where agents run again.`
      : 'Your sign-in expired. Sign in again, then open Where agents run.'
  }

  if (status === 403) {
    return isSaveAction
      ? `${saveBase} Ask an owner or admin for access to change where agents run.`
      : 'Ask an owner or admin for access to change where agents run.'
  }

  if (status === 404) {
    return isSaveAction
      ? `${saveBase} Refresh after the Where agents run settings are available.`
      : 'Refresh Settings after the Where agents run settings are available.'
  }

  if (status === 409) {
    return isSaveAction
      ? `${saveBase} The Where agents run choices changed while you were working. Refresh Settings, review the current choices, then save again.`
      : 'The Where agents run choices changed while you were working. Refresh Settings, review the current choices, then try again.'
  }

  if (status === 422) {
    return `${saveBase} Choose an available agent location and work tool, then save again.`
  }

  if (status === 429) {
    return isSaveAction
      ? `${saveBase} Too many setup requests are happening right now. Wait a minute, then save again.`
      : 'Too many setup requests are happening right now. Wait a minute, then refresh Settings.'
  }

  if (status && status >= 500) {
    return isSaveAction
      ? `${saveBase} Refresh Settings, then save again. If it still fails, ask an owner or admin to check Where agents run.`
      : `${loadBase} If it still fails, ask an owner or admin to check Where agents run.`
  }

  return isSaveAction
    ? `${saveBase} Try again. If it still fails, ask an owner or admin to check Where agents run.`
    : `${loadBase} If it still fails, ask an owner or admin to check Where agents run.`
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
    return 'Work tool sign-in could not be checked. Refresh this page, then reconnect the work tool sign-in.'
  }

  return 'Agent connection status could not load. Start or wake an agent, then refresh this page.'
}
