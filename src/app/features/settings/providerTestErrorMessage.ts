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

export function providerTestErrorMessage(error: unknown, providerName = 'Provider'): string {
  const base = `${providerName} connection test failed.`
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || code === 403 || text.includes('unauthorized') || text.includes('forbidden')) {
    return `${base} Check that the saved API key is active and allowed to use the selected model, then save and test again.`
  }
  if (
    code === 400 ||
    code === 422 ||
    text.includes('invalid key') ||
    text.includes('api key') ||
    text.includes('authentication')
  ) {
    return `${base} Check the API key, model, and Base URL, then save and test again.`
  }
  if (code === 404 || text.includes('not found')) {
    return `${base} The model or provider endpoint was not found. Check the model name and Base URL, then test again.`
  }
  if (code === 408 || code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `${base} The provider is busy or rate limiting tests. Wait a minute, then test again.`
  }
  if (code != null && code >= 500) {
    return `${base} The provider service or gateway is temporarily unavailable. Try again in a few minutes.`
  }
  if (isNetworkError(error)) {
    return `${base} The platform could not reach the provider. Check network access and the Base URL, then test again.`
  }

  return `${base} Review the provider settings, then test again.`
}
