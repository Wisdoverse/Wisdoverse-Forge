type ProviderSettingsAction = 'load' | 'save' | 'remove'

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

function actionFromText(text: string): ProviderSettingsAction {
  const lower = text.toLowerCase()
  if (/\b(delete|deleted|remove|removed|removing)\b/i.test(text)) return 'remove'
  if (
    /\b(save|saved|saving|create|created|creating|update|updated|updating)\b/i.test(text) ||
    lower.includes('required fields for provider') ||
    lower.includes('api key is required') ||
    lower.includes('model is required') ||
    lower.includes('base url is required') ||
    lower.includes('base_url is required') ||
    lower.includes('invalid provider') ||
    lower.includes('already exists') ||
    lower.includes('duplicate')
  ) {
    return 'save'
  }
  return 'load'
}

function retryAction(action: ProviderSettingsAction): string {
  if (action === 'save') return 'save this AI service again'
  if (action === 'remove') return 'remove this AI service again'
  return 'open Settings and AI services again'
}

function validationGuidance(lower: string): string {
  if (lower.includes('api key') || lower.includes('token') || lower.includes('key')) {
    return 'Paste the service access key from the selected AI service, then save again.'
  }
  if (lower.includes('model')) {
    return 'Keep the suggested service choice or choose the choice name from your service guide, then save again.'
  }
  if (lower.includes('base url') || lower.includes('base_url')) {
    return 'Add the service address for this AI service, then save again.'
  }
  if (lower.includes('provider')) {
    return 'Choose an AI service from the list, then save again.'
  }
  return 'Choose the AI service, keep the suggested service choice, add the service access key if needed, then save again.'
}

export function providerSettingsErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const retry = retryAction(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `Sign in again, then ${retry}. Your sign-in expired.`
  }
  if (
    code === 403 ||
    lower.includes('permission') ||
    lower.includes('forbidden') ||
    lower.includes('role required')
  ) {
    return 'Ask an owner or admin to let you manage AI services.'
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return 'Open Settings and AI services again, check the current AI service, then choose a different name or remove the old service first.'
  }
  if (
    code === 422 ||
    lower.includes('api key') ||
    lower.includes('model is required') ||
    lower.includes('base url') ||
    lower.includes('invalid provider')
  ) {
    return validationGuidance(lower)
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return `Wait a minute, then ${retry}. Forge is receiving too many AI service requests right now.`
  }
  if (code != null && code >= 500) {
    if (action === 'load') {
      return 'Open Settings and AI services again. If it still fails, ask an owner or admin to check AI service settings.'
    }
    return `Open Settings and AI services again, then ${retry}. If it still fails, ask an owner or admin to check AI service settings.`
  }
  if (isNetworkError(error)) {
    if (action === 'load') {
      return 'Check your connection, then open Settings and AI services again. Forge could not connect while opening AI service settings.'
    }
    if (action === 'remove') {
      return 'Check your connection, then remove this AI service again. The removal did not finish.'
    }
    return 'Check your connection, then save this AI service again. The save did not finish.'
  }

  if (action === 'load') {
    return 'Open Settings and AI services again. If it still fails, ask an owner or admin to check AI service settings.'
  }

  return `${retry.charAt(0).toUpperCase()}${retry.slice(1)}. If it still fails, ask an owner or admin to check AI service settings.`
}
