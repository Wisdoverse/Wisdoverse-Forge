export type RuntimeErrorAction = 'loadAgentSignals' | 'loadCliSignIn' | 'startCliSignIn'

const ACTION_FALLBACKS: Record<RuntimeErrorAction, string> = {
  loadAgentSignals:
    'Agent connection status could not load. Start or wake an agent, then refresh this page.',
  loadCliSignIn:
    'Tool account connection could not be checked. Refresh this page before starting agents that use work tools.',
  startCliSignIn:
    'Tool account connection did not start. Check the connected AI service, then reconnect the account.',
}

export function runtimeErrorMessage(action: RuntimeErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} Forge could not connect while checking Agent Work Setup. Check your connection, then refresh Settings.`
  }

  if (status === 401) {
    return 'Your sign-in expired. Sign in again, then open Agent Work Setup and try again.'
  }

  if (status === 403) {
    return 'You do not have permission to manage Agent Work Setup. Ask an owner or admin to update your role.'
  }

  if (status === 404) {
    return 'Agent Work Setup is not available yet. Refresh Settings. If it still does not load, ask an owner or admin to check it.'
  }

  if (status === 409) {
    return 'Agent Work Setup changed while you were working. Refresh this page, review the current status, then try again.'
  }

  if (status === 422) {
    return runtimeValidationMessage(action, detail)
  }

  if (status === 429) {
    return 'Forge is receiving too many Agent Work Setup requests right now. Wait a moment, then try again.'
  }

  if (status && status >= 500) {
    return 'Forge could not check Agent Work Setup right now. Refresh this page, then try again. If it still fails, ask an owner or admin to check Agent Work Setup.'
  }

  return runtimeValidationMessage(action, detail)
}

export function runtimeSettingsErrorMessage(err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)
  const base =
    normalized.includes('update') ||
    normalized.includes('required fields for runtime setting') ||
    normalized.includes('default cli tool') ||
    normalized.includes('default runtime') ||
    normalized.includes('not available')
      ? 'Agent Work Setup could not be saved.'
      : 'Agent Work Setup could not be loaded.'

  if (isNetworkError(normalized)) {
    return `${base} Forge could not connect while opening Agent Work Setup. Check your connection, then refresh Settings.`
  }

  if (status === 401) {
    return `${base} Your sign-in expired. Sign in again, then open Agent Work Setup and try again.`
  }

  if (status === 403) {
    return `${base} Ask an owner or admin for access to manage Agent Work Setup.`
  }

  if (status === 404) {
    return `${base} Refresh after Agent Work Setup is available.`
  }

  if (status === 409) {
    return `${base} Agent Work Setup changed while you were working. Refresh Settings, review the current choices, then try again.`
  }

  if (status === 422) {
    return `${base} Choose an available work location and local tool, then save again.`
  }

  if (status === 429) {
    return `${base} Forge is receiving too many Agent Work Setup requests right now. Wait a minute, then try again.`
  }

  if (status && status >= 500) {
    return `${base} Refresh Settings, then try again. If it still fails, ask an owner or admin to check Agent Work Setup.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check Agent Work Setup.`
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

function runtimeValidationMessage(action: RuntimeErrorAction, detail: string): string {
  const normalized = detail.toLowerCase()

  if (action === 'startCliSignIn') {
    if (normalized.includes('provider') || normalized.includes('configured')) {
      return 'Choose and save an AI service first, then reconnect the tool account.'
    }
    if (normalized.includes('tool') || normalized.includes('cli')) {
      return 'Choose an available local tool, then reconnect the tool account.'
    }
    return 'Check the connected AI service and selected local tool, then reconnect the tool account.'
  }

  if (action === 'loadCliSignIn') {
    return 'Tool account connection could not be checked. Refresh this page, then reconnect the tool account.'
  }

  return 'Agent connection status could not load. Start or wake an agent, then refresh this page.'
}
