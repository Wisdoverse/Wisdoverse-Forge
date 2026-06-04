export type RuntimeErrorAction = 'loadAgentSignals' | 'loadCliSignIn' | 'startCliSignIn'

const ACTION_FALLBACKS: Record<RuntimeErrorAction, string> = {
  loadAgentSignals:
    'Agent online status could not load. Start or wake an agent, then refresh this setup check.',
  loadCliSignIn:
    'Local tool sign-in status could not load. Refresh this setup check before starting local-tool agents.',
  startCliSignIn:
    'Local tool sign-in did not start. Check the provider setup, then try Connect again.',
}

export function runtimeErrorMessage(action: RuntimeErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    return `${ACTION_FALLBACKS[action]} The app could not reach the service. Check your connection, then refresh the page.`
  }

  if (status === 401) {
    return 'Sign in again, then open Agent setup and try this action again.'
  }

  if (status === 403) {
    return 'You do not have permission to manage agent setup. Ask an owner or admin to update your role.'
  }

  if (status === 404) {
    return 'Agent setup is not available yet. Refresh after the service is ready.'
  }

  if (status === 409) {
    return 'Agent setup changed while you were working. Refresh this setup check, review the current status, then try again.'
  }

  if (status === 422) {
    return runtimeValidationMessage(action, detail)
  }

  if (status === 429) {
    return 'Agent setup is busy with too many requests. Wait a moment, then try again.'
  }

  if (status && status >= 500) {
    return 'Agent setup is temporarily unavailable. Refresh this setup check, then try again. If it still fails, ask an owner or admin to check the agent setup service.'
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
      ? 'Agent work settings could not be saved.'
      : 'Agent work settings could not be loaded.'

  if (isNetworkError(normalized)) {
    return `${base} The app could not reach the service. Check your connection, then refresh Settings.`
  }

  if (status === 401) {
    return `${base} Sign in again, then open Settings and try agent setup again.`
  }

  if (status === 403) {
    return `${base} Ask an owner or admin for access to manage agent setup.`
  }

  if (status === 404) {
    return `${base} Refresh after agent work settings are available.`
  }

  if (status === 409) {
    return `${base} Agent setup changed while you were working. Refresh Settings, review the current choices, then try again.`
  }

  if (status === 422) {
    return `${base} Choose an available work location and local tool, then save again.`
  }

  if (status === 429) {
    return `${base} The service is busy. Wait a minute, then try again.`
  }

  if (status && status >= 500) {
    return `${base} The agent work settings service is temporarily unavailable. Refresh Settings, then try again. If it still fails, ask an owner to check agent work settings.`
  }

  return `${base} Try again. If it still fails, ask an owner to check agent work settings.`
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
      return 'Choose and save a provider first, then try Connect again.'
    }
    if (normalized.includes('tool') || normalized.includes('cli')) {
      return 'Choose an available local tool, then try Connect again.'
    }
    return 'Check the provider setup and selected local tool, then try Connect again.'
  }

  if (action === 'loadCliSignIn') {
    return 'Local tool sign-in status could not load. Refresh this setup check, then connect the local tool again.'
  }

  return 'Agent online status could not load. Start or wake an agent, then refresh this setup check.'
}
