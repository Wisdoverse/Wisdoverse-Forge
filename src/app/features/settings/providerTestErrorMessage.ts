function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  if (!error || typeof error !== 'object') return ''

  const value = error as {
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

function statusCode(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const value = error as { status?: unknown; statusCode?: unknown; code?: unknown }
    for (const candidate of [value.status, value.statusCode, value.code]) {
      const code = numericStatus(candidate)
      if (code) return code
    }
  }

  const match = errorText(error).match(/\b(?:HTTP|API|Server error)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function numericStatus(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && /^\d+$/.test(value)) return Number(value)
  return null
}

function isNetworkError(error: unknown): boolean {
  const text = errorText(error).toLowerCase()
  return (
    error instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('connection refused') ||
    text.includes('timeout')
  )
}

export function providerTestErrorMessage(error: unknown, providerName = 'AI service'): string {
  const base = `${providerName} connection check needs attention.`
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || code === 403 || text.includes('unauthorized') || text.includes('forbidden')) {
    return `${base} Confirm the saved service access key is active and allowed to use the selected model, then save and check again.`
  }
  if (
    code === 400 ||
    code === 422 ||
    text.includes('invalid key') ||
    text.includes('api key') ||
    text.includes('authentication')
  ) {
    return `${base} Check the service access key, model, and service address, then save and check again.`
  }
  if (code === 404 || text.includes('not found')) {
    return `${base} The model or service address was not found. Check the model name and service address, then check again.`
  }
  if (code === 408 || code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `${base} This AI service is receiving too many checks right now. Wait a minute, then check again.`
  }
  if (code != null && code >= 500) {
    return `${base} Forge could not check this AI service right now. Try again in a few minutes. If it still needs attention, ask an owner or admin to check AI service settings.`
  }
  if (isNetworkError(error)) {
    return `${base} Forge could not connect to this AI service. Check the service address and your connection, then check again.`
  }

  return `${base} Review the AI service settings, then check again. If it still needs attention, ask an owner or admin to check AI service settings.`
}
