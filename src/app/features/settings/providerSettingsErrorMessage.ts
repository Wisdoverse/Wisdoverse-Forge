type ProviderSettingsAction = 'load' | 'save' | 'remove'

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

function actionFromText(text: string): ProviderSettingsAction {
  const lower = text.toLowerCase()
  if (/\b(delete|deleted|remove|removed|removing)\b/i.test(text)) return 'remove'
  if (
    /\b(save|saved|saving|create|created|creating|update|updated|updating)\b/i.test(text) ||
    lower.includes('required fields for provider') ||
    lower.includes('api key is required') ||
    lower.includes('model is required') ||
    lower.includes('base url is required') ||
    lower.includes('invalid provider') ||
    lower.includes('already exists') ||
    lower.includes('duplicate')
  ) {
    return 'save'
  }
  return 'load'
}

function baseMessage(action: ProviderSettingsAction): string {
  if (action === 'save') return 'Model service could not be saved.'
  if (action === 'remove') return 'Model service could not be removed.'
  return 'Model service settings could not be loaded.'
}

export function providerSettingsErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const base = baseMessage(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `${base} Sign in again, then open Settings and try model services again.`
  }
  if (code === 403 || lower.includes('permission') || lower.includes('forbidden')) {
    return `${base} Ask an owner or admin to let you manage model services.`
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return `${base} A model service with this name or setup already exists. Refresh the list, then choose a different name or remove the old service first.`
  }
  if (
    code === 422 ||
    lower.includes('api key') ||
    lower.includes('model is required') ||
    lower.includes('base url') ||
    lower.includes('invalid provider')
  ) {
    return `${base} Choose the AI service, confirm the model, add the service access key, and add the service address if needed. Then save again.`
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return `${base} The service is busy. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The model service settings page is temporarily unavailable. Try again. If it still fails, ask an owner to check model service settings.`
  }
  if (isNetworkError(error)) {
    return `${base} The app could not reach the service. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner to check model service settings.`
}
