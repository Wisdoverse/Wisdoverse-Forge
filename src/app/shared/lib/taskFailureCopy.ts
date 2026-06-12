export function taskFailurePreview(error?: string | null): string {
  const message = error?.trim() ?? ''
  if (!message) return 'Stopped before finishing. Open details for next steps.'

  const lowerMessage = message.toLowerCase()
  if (lowerMessage.includes('rate limit') || /\b429\b/.test(message)) {
    return 'Stopped because the AI service is busy. Open details to retry when ready.'
  }
  if (lowerMessage.includes('timeout') || lowerMessage.includes('timed out')) {
    return 'Stopped because the task took too long. Open details to retry.'
  }
  if (
    lowerMessage.includes('permission') ||
    lowerMessage.includes('forbidden') ||
    /\b403\b/.test(message)
  ) {
    return 'Stopped because access is missing. Ask an owner or admin for help.'
  }
  if (lowerMessage.includes('unauthorized') || /\b401\b/.test(message)) {
    return 'Stopped because sign-in or service access needs attention.'
  }

  return 'Stopped before finishing. Open details to see what happened and retry.'
}

interface TaskBlockedPreviewInput {
  blockedHint?: string | null
  blockedReason?: string | null
  error?: string | null
}

const GENERIC_BLOCKED_PREVIEW =
  'This task needs your input before it can continue. Open details for next steps.'

export function taskBlockedPreview({
  blockedHint,
  blockedReason,
  error,
}: TaskBlockedPreviewInput): string {
  const hint = blockedHint?.trim()
  if (hint) return beginnerBlockedHint(hint)

  switch (blockedReason) {
    case 'waiting_agent':
      return 'Choose or free an agent, then send the task again.'
    case 'waiting_dependency':
      return 'Finish the earlier dependency, then check this task again.'
    case 'waiting_input':
      return 'Add the missing information so the agent can continue.'
    case 'waiting_approval':
      return 'Review the approval request, then approve or decline it.'
    case 'quota_exceeded':
      return 'Pause lower-priority work or ask an owner to raise the limit, then retry.'
    default:
      return blockedErrorPreview(error)
  }
}

function beginnerBlockedHint(hint: string): string {
  if (/\b(api\s*)?(credential|credentials|key|keys|token|tokens|secret|secrets)\b/i.test(hint)) {
    return 'Waiting for account access. Add or reconnect the required service access, then retry.'
  }
  return hint
}

function blockedErrorPreview(error?: string | null): string {
  const detail = error?.trim().toLowerCase() ?? ''
  if (!detail) return GENERIC_BLOCKED_PREVIEW

  if (
    detail.includes('credential') ||
    detail.includes('token') ||
    detail.includes('secret') ||
    detail.includes('auth') ||
    detail.includes('unauthorized') ||
    /\b401\b/.test(detail)
  ) {
    return 'This task needs account access before it can continue. Reconnect access or ask an owner for help.'
  }
  if (detail.includes('permission') || detail.includes('forbidden') || /\b403\b/.test(detail)) {
    return 'This task needs access before it can continue. Ask an owner or admin for help.'
  }
  if (detail.includes('quota') || detail.includes('rate limit') || /\b429\b/.test(detail)) {
    return 'The workspace is busy. Retry later or ask an owner for help.'
  }
  if (detail.includes('timeout') || detail.includes('timed out')) {
    return 'A required service did not answer in time. Open details and retry when it is ready.'
  }

  return GENERIC_BLOCKED_PREVIEW
}
