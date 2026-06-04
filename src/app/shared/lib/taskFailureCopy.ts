export function taskFailurePreview(error?: string | null): string {
  const message = error?.trim() ?? ''
  if (!message) return 'Stopped before finishing. Open details for next steps.'

  const lowerMessage = message.toLowerCase()
  if (lowerMessage.includes('rate limit') || /\b429\b/.test(message)) {
    return 'Stopped because the model service is busy. Open details to retry when ready.'
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
