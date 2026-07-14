export function waitingPlaceDisplayName(name: string | null | undefined): string {
  const trimmed = name?.trim()
  if (!trimmed) return 'this place'

  return trimmed
    .replace(/\bwaiting\s+places?\b/gi, 'place')
    .replace(/\btask\s+queues?\b/gi, 'place')
    .replace(/\bqueues?\b/gi, 'place')
    .replace(/\btask\s+place\b/gi, 'place')
}
