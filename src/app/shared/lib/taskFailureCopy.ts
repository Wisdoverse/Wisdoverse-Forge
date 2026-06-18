export function taskFailurePreview(error?: string | null): string {
  const message = error?.trim() ?? ''
  if (!message)
    return 'Stopped before finishing. Open the task details, review the latest update, then retry when ready.'

  const lowerMessage = message.toLowerCase()
  if (lowerMessage.includes('rate limit') || /\b429\b/.test(message)) {
    return 'Stopped because the AI service is busy. Wait a minute, then open the task details and retry.'
  }
  if (lowerMessage.includes('timeout') || lowerMessage.includes('timed out')) {
    return 'Stopped because the task took too long. Open the task details, review the latest update, then retry when ready.'
  }
  if (
    lowerMessage.includes('permission') ||
    lowerMessage.includes('forbidden') ||
    /\b403\b/.test(message)
  ) {
    return 'Stopped because access is missing. Ask an owner or admin for help.'
  }
  if (lowerMessage.includes('unauthorized') || /\b401\b/.test(message)) {
    return 'Reconnect sign-in or service access, then retry.'
  }

  return 'Stopped before finishing. Open the task details, review the latest update, then retry when ready.'
}

export function isRawTaskFailureDetail(message: string): boolean {
  const trimmed = message.trim()
  const raw = trimmed.toLowerCase()

  return (
    /\b(?:command\s+)?exited?\s+\d+\b/.test(raw) ||
    /\bexit\s+\d+\b/.test(raw) ||
    /\b(?:http|api)\s+\d{3}\b/.test(raw) ||
    /\b(?:panic|stack trace|traceback|exception|stdout|stderr|raw command output|database)\b/i.test(
      trimmed
    ) ||
    raw.includes('unauthorized') ||
    raw.includes('non-zero') ||
    raw.includes('provider') ||
    /\b(?:credential|credentials|key|keys|token|tokens|secret|secrets)\b/i.test(trimmed)
  )
}

interface TaskBlockedPreviewInput {
  blockedHint?: string | null
  blockedReason?: string | null
  error?: string | null
}

const GENERIC_BLOCKED_PREVIEW =
  'This task needs your input before it can continue. Open the task details, read the latest update, then add the missing answer or ask an owner for help.'

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
  if (/\b(?:quota|rate limit|rate limited)\b/i.test(hint) || /\b429\b/.test(hint)) {
    return 'Too much work is running right now. Wait a bit, then retry or ask an owner for help.'
  }
  if (/\b(api\s*)?(credential|credentials|key|keys|token|tokens|secret|secrets)\b/i.test(hint)) {
    return 'Waiting for account access. Add or reconnect the required service access, then retry.'
  }
  if (containsTechnicalBlockedHint(hint)) {
    return 'This task needs help before it can continue. Open the task details, review the latest update, then retry or ask an owner for help.'
  }
  return hint
}

function containsTechnicalBlockedHint(hint: string): boolean {
  return /\b(panic|stack trace|traceback|exception|stdout|stderr|raw command output|docker socket|internal error|database)\b/i.test(
    hint
  )
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
    return 'Too much work is running right now. Wait a bit, then retry or ask an owner for help.'
  }
  if (detail.includes('timeout') || detail.includes('timed out')) {
    return 'A required service did not answer in time. Open the task details and retry when it is ready.'
  }

  return GENERIC_BLOCKED_PREVIEW
}
