function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  return typeof error === 'string' ? error : ''
}

function statusCode(error: unknown): number | null {
  if (error && typeof error === 'object' && 'statusCode' in error) {
    const statusCode = (error as { statusCode?: unknown }).statusCode
    if (typeof statusCode === 'number') return statusCode
  }

  const match = errorText(error).match(/\b(?:HTTP|API|Server error)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
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

export function providerTestErrorMessage(error: unknown, providerName = 'Model service'): string {
  const base = `${providerName} connection test failed.`
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || code === 403 || text.includes('unauthorized') || text.includes('forbidden')) {
    return `${base} Check that the saved service access key is active and allowed to use the selected model, then save and check again.`
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
    return `${base} The model service is busy or limiting checks. Wait a minute, then check again.`
  }
  if (code != null && code >= 500) {
    return `${base} The model service or gateway is temporarily unavailable. Try again in a few minutes.`
  }
  if (isNetworkError(error)) {
    return `${base} The platform could not reach the model service. Check network access and the service address, then check again.`
  }

  return `${base} Review the model service settings, then check again.`
}
