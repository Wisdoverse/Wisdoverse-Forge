export function waitingPlaceDisplayName(name: string | null | undefined): string {
  const trimmed = name?.trim()
  if (!trimmed) return 'this place'

  return trimmed
    .replace(/\btask\s+queues?\b/gi, 'waiting place')
    .replace(/\bqueues?\b/gi, 'waiting place')
}
