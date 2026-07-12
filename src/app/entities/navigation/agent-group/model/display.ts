export function waitingPlaceDisplayName(name: string | null | undefined): string {
  const trimmed = name?.trim()
  if (!trimmed) return 'this task queue'

  return trimmed
    .replace(/\bwaiting\s+places?\b/gi, 'task queue')
    .replace(/\btask\s+queues?\b/gi, 'task queue')
    .replace(/\bqueues?\b/gi, 'task queue')
    .replace(/\btask\s+task\s+queue\b/gi, 'task queue')
}
