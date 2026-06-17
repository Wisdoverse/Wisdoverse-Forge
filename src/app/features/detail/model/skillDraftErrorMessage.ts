function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  return typeof error === 'string' ? error : ''
}

function structuredErrorText(error: unknown): string {
  if (!error || typeof error !== 'object') return errorText(error)
  for (const key of ['serverError', 'detail', 'error', 'message', 'reason'] as const) {
    const value = (error as Record<string, unknown>)[key]
    if (typeof value === 'string' && value.trim()) return value
  }
  return errorText(error)
}

function statusCode(error: unknown): number | null {
  if (error && typeof error === 'object') {
    for (const key of ['statusCode', 'status', 'code'] as const) {
      const value = (error as Record<string, unknown>)[key]
      if (typeof value === 'number' && Number.isFinite(value)) return value
      if (typeof value === 'string' && /^\d{3}$/.test(value.trim())) {
        return Number.parseInt(value, 10)
      }
    }
  }

  const match = structuredErrorText(error).match(
    /\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i
  )
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(error: unknown): boolean {
  const text = structuredErrorText(error).toLowerCase()
  return (
    error instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('load failed')
  )
}

export function skillDraftErrorMessage(error: unknown): string {
  const failure = 'Instruction was not published.'
  const text = structuredErrorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || text.includes('unauthorized') || text.includes('sign in again')) {
    return `Sign in again, reopen this task, and publish the instruction again. ${failure}`
  }
  if (
    code === 403 ||
    text.includes('forbidden') ||
    text.includes('permission') ||
    text.includes('let you create saved instructions') ||
    text.includes('cannot create workspace instructions')
  ) {
    return `Ask an owner or admin to let you create saved instructions, then publish again. ${failure}`
  }
  if (code === 404) {
    return `Refresh the task, then publish the instruction again. ${failure} Instruction publishing setup may have changed.`
  }
  if (
    code === 409 ||
    text.includes('already exists') ||
    text.includes('already exist') ||
    text.includes('duplicate')
  ) {
    return `Rename it, then publish again. An instruction with this name may already exist. ${failure}`
  }
  if (code === 422 || text.includes('validation')) {
    return `Check the name, trigger words, and reusable instructions, then publish again. ${failure}`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `Wait a minute, then publish again. Too many instruction changes are happening right now. ${failure}`
  }
  if (code != null && code >= 500) {
    return 'Wait a few minutes, then publish again. Forge could not publish this instruction right now. If it still fails, ask an owner or admin to check instruction setup.'
  }
  if (isNetworkError(error)) {
    return 'Check your connection, then publish again. Forge could not connect while publishing this instruction.'
  }

  return `Review the draft, then publish again. ${failure} If it still fails, ask an owner or admin to check instruction setup.`
}
