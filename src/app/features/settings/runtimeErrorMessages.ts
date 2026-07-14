export type RuntimeErrorAction = 'loadAgentSignals' | 'loadCliSignIn' | 'startCliSignIn'

const RAW_SERVICE_DETAIL =
  /\b(database|sql|stack trace|traceback|exception|panic|internal server error)\b/i

const ACTION_FALLBACKS: Record<RuntimeErrorAction, string> = {
  loadAgentSignals:
    'Open Agents and make sure one agent shows Ready, then open Settings and Where agents work again. Agent connection status could not load.',
  loadCliSignIn:
    'Open Settings, then Codex sign-in again before starting agents that use code tools. Code tool sign-in could not be checked.',
  startCliSignIn:
    'Open Settings, then Codex sign-in again, then reconnect the account. Code tool sign-in did not start.',
}

const ACTION_RECOVERY: Record<
  RuntimeErrorAction,
  { location: string; openStep: string; target: string }
> = {
  loadAgentSignals: {
    location: 'Where agents work',
    openStep: 'open Settings and Where agents work',
    target: 'Where agents work',
  },
  loadCliSignIn: {
    location: 'Codex sign-in',
    openStep: 'open Settings, then Codex sign-in',
    target: 'the Codex sign-in page',
  },
  startCliSignIn: {
    location: 'Codex sign-in',
    openStep: 'open Settings, then Codex sign-in',
    target: 'the Codex sign-in page',
  },
}

export function runtimeErrorMessage(action: RuntimeErrorAction, err: unknown): string {
  const detail = errorDetail(err)
  const normalized = detail.toLowerCase()
  const status = errorStatus(err, normalized)

  if (isNetworkError(normalized)) {
    const recovery = ACTION_RECOVERY[action]
    return `${ACTION_FALLBACKS[action]} Check your connection, then ${recovery.openStep} again. Forge could not connect while checking ${recovery.target}.`
  }

  if (status === 401) {
    const recovery = ACTION_RECOVERY[action]
    const retryStep =
      action === 'startCliSignIn'
        ? `${recovery.openStep} again, then reconnect the account`
        : `${recovery.openStep} again`
    return `Sign in again, then ${retryStep}. Your sign-in expired.`
  }

  if (status === 403) {
    const recovery = ACTION_RECOVERY[action]
    return `Ask an owner or admin to update your team space access before changing ${recovery.location}. You do not have permission to change ${recovery.location}.`
  }

  if (status === 404) {
    const recovery = ACTION_RECOVERY[action]
    return `${sentenceCase(recovery.openStep)} again. ${recovery.location} is not available yet. If it still does not load, ask an owner or admin to check it.`
  }

  if (status === 409) {
    const recovery = ACTION_RECOVERY[action]
    return `${ACTION_FALLBACKS[action]} The choices in ${recovery.location} changed while you were working.`
  }

  if (status === 422) {
    return runtimeValidationMessage(action, detail)
  }

  if (status === 429) {
    const recovery = ACTION_RECOVERY[action]
    const retryStep =
      action === 'startCliSignIn'
        ? `${recovery.openStep} again, then reconnect the account`
        : `${recovery.openStep} again`
    return `Wait a minute, then ${retryStep}. Forge is receiving too many setup requests right now.`
  }

  if ((status && status >= 500) || (!status && RAW_SERVICE_DETAIL.test(normalized))) {
    const recovery = ACTION_RECOVERY[action]
    const retryStep =
      action === 'startCliSignIn'
        ? `${recovery.openStep} again, then reconnect the account`
        : `${recovery.openStep} again`
    return `${sentenceCase(retryStep)}. Forge could not check ${recovery.target} right now. If it still fails, ask an owner or admin to check ${recovery.location} in Settings.`
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
  const loadBase = 'Open Settings and Where agents work again.'

  if (isNetworkError(normalized)) {
    return isSaveAction
      ? 'Check your connection, then save Where agents work again. Forge could not connect while saving Where agents work.'
      : 'Check your connection, then open Settings and Where agents work again.'
  }

  if (status === 401) {
    return isSaveAction
      ? 'Sign in again, then save Where agents work again. Your sign-in expired.'
      : 'Sign in again, then open Where agents work. Your sign-in expired.'
  }

  if (
    status === 403 ||
    normalized.includes('permission') ||
    normalized.includes('forbidden') ||
    normalized.includes('role required')
  ) {
    return isSaveAction
      ? 'Ask an owner or admin for access to change Where agents work, then save again. Where agents work could not be saved.'
      : 'Ask an owner or admin for access to change Where agents work.'
  }

  if (status === 404) {
    return isSaveAction
      ? 'Open Settings and Where agents work again, then save after Where agents work is available. Where agents work could not be saved.'
      : 'Open Settings and Where agents work again after Where agents work is available.'
  }

  if (status === 409) {
    return isSaveAction
      ? 'Open Settings and Where agents work again, check the current choices, then save again. The choices in Where agents work changed while you were working.'
      : 'Open Settings and Where agents work again, then check the current choices. The choices in Where agents work changed while you were working.'
  }

  if (status === 422) {
    return 'Choose where project files open and a work tool, then save again. Where agents work could not be saved.'
  }

  if (status === 429) {
    return isSaveAction
      ? 'Wait a minute, then save Where agents work again. Too many setup requests are happening right now.'
      : 'Wait a minute, then open Settings and Where agents work again. Too many setup requests are happening right now.'
  }

  if ((status && status >= 500) || (!status && RAW_SERVICE_DETAIL.test(normalized))) {
    return isSaveAction
      ? 'Open Settings and Where agents work again, then save again. Where agents work could not be saved. If it still fails, ask an owner or admin to check Where agents work in Settings.'
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
    const detail = payloadDetail(candidate)
    if (detail) return detail
  }

  return ''
}

function payloadDetail(value: unknown): string | null {
  if (typeof value === 'string' && value.trim()) return value.trim()
  if (!value || typeof value !== 'object') return null

  const record = value as Record<string, unknown>
  for (const key of ['serverError', 'message', 'error', 'detail', 'reason']) {
    const detail = payloadDetail(record[key])
    if (detail) return detail
  }

  return null
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
      return 'Choose and save an AI service first, then reconnect the code tool sign-in.'
    }
    if (normalized.includes('tool') || normalized.includes('cli')) {
      return 'Choose an available code tool, then reconnect the code tool sign-in.'
    }
    return 'Check the connected AI service and selected code tool, then reconnect the code tool sign-in.'
  }

  if (action === 'loadCliSignIn') {
    return 'Open Settings, then Codex sign-in again, then reconnect the code tool sign-in. Code tool sign-in could not be checked.'
  }

  return 'Open Agents and make sure one agent shows Ready, then open Settings and Where agents work again. Agent connection status could not load.'
}

function sentenceCase(value: string): string {
  return value.length === 0 ? value : `${value[0].toUpperCase()}${value.slice(1)}`
}
