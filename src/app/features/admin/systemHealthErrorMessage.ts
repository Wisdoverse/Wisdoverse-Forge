function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  return typeof error === 'string' ? error : ''
}

function statusCode(error: unknown): number | null {
  const match = errorText(error).match(/\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i)
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
    text.includes('browser could not reach') ||
    text.includes('load failed')
  )
}

export function systemHealthErrorMessage(error: unknown): string {
  const base = 'Service readiness could not be loaded.'
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || text.includes('sign in again') || text.includes('unauthorized')) {
    return `${base} Sign in again, then open Admin and check service readiness.`
  }
  if (code === 403 || text.includes('permission') || text.includes('forbidden')) {
    return `${base} Ask an owner to give you admin access for service readiness.`
  }
  if (code === 404 || text.includes('endpoint is not available')) {
    return `${base} Refresh after the backend with the health endpoint is deployed.`
  }
  if (code === 429 || text.includes('busy') || text.includes('too many')) {
    return `${base} The admin API is busy. Wait a minute, then choose Check now.`
  }
  if (code != null && code >= 500) {
    return `${base} The admin API is temporarily unavailable. Check the backend service, then choose Check now.`
  }
  if (isNetworkError(error)) {
    return `${base} The browser could not reach the server. Check your connection or API route, then choose Check now.`
  }

  return `${base} Choose Check now again. If it still fails, ask an owner to check the admin API.`
}
