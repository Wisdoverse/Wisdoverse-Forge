function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  if (!error || typeof error !== 'object') return ''

  const value = error as {
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

function statusCode(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const value = error as { status?: unknown; statusCode?: unknown; code?: unknown }
    for (const candidate of [value.status, value.statusCode, value.code]) {
      const code = numericStatus(candidate)
      if (code) return code
    }
  }

  const match = errorText(error).match(/\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i)
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
    text.includes('browser could not reach') ||
    text.includes('load failed')
  )
}

export function systemHealthErrorMessage(error: unknown): string {
  const base = 'Forge could not check service readiness.'
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || text.includes('sign in again') || text.includes('unauthorized')) {
    return `${base} Your sign-in expired. Sign in again, then open Admin and choose Check now.`
  }
  if (code === 403 || text.includes('permission') || text.includes('forbidden')) {
    return `${base} You do not have access to service readiness. Ask an owner or admin to update your role, then choose Check now.`
  }
  if (code === 404 || text.includes('endpoint is not available')) {
    return `${base} Service readiness is not available from this Admin view. Refresh Admin, then choose Check now. If it still fails, ask an owner or admin to check setup.`
  }
  if (code === 429 || text.includes('busy') || text.includes('too many')) {
    return `${base} Forge is receiving too many readiness checks right now. Wait a minute, then choose Check now.`
  }
  if (code != null && code >= 500) {
    return `${base} Refresh Admin, then choose Check now. If it still fails, ask an owner or admin to check service readiness setup.`
  }
  if (isNetworkError(error)) {
    return `${base} Forge could not connect while checking service readiness. Check your connection, then choose Check now.`
  }

  return `${base} Choose Check now again. If it still fails, ask an owner or admin to check service readiness setup.`
}
