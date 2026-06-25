// F069: recovery for failed dynamic imports.
//
// Right after a deploy the already-loaded index.html references the OLD chunk
// hashes; navigating to a lazy route then 404s the old chunk and throws. A
// single full reload fetches the new index + new chunk hashes and recovers. We
// guard the reload with a session flag so a genuinely-broken chunk cannot
// reload-loop.

const RELOAD_FLAG = 'af:chunk-reload-attempted'

/** True when `error` looks like a failed dynamic-import / chunk-load. */
export function isChunkLoadError(error: unknown): boolean {
  if (!error) return false
  const name = error instanceof Error ? error.name : ''
  const message = error instanceof Error ? error.message : String(error)
  return (
    name === 'ChunkLoadError' ||
    /failed to fetch dynamically imported module/i.test(message) ||
    /error loading dynamically imported module/i.test(message) ||
    /loading chunk [\d]+ failed/i.test(message) ||
    /importing a module script failed/i.test(message)
  )
}

/**
 * Reload the page. A seam so callers and tests never touch `window.location`
 * directly (jsdom's `location.reload` is non-configurable and cannot be spied).
 */
export function reloadPage(): void {
  if (typeof window !== 'undefined') window.location.reload()
}

/**
 * If `error` is a chunk-load failure and we have not already reloaded this
 * session, reload the page once and return `true`. Otherwise return `false` so
 * the caller renders a recovery UI instead of looping. `reload` is injectable
 * for tests; production uses the default page reload.
 */
export function recoverFromChunkError(error: unknown, reload: () => void = reloadPage): boolean {
  if (typeof window === 'undefined' || !isChunkLoadError(error)) return false
  try {
    if (sessionStorage.getItem(RELOAD_FLAG)) return false
    sessionStorage.setItem(RELOAD_FLAG, '1')
  } catch {
    // No sessionStorage → cannot guard against a loop, so do NOT auto-reload.
    return false
  }
  reload()
  return true
}

/** Clear the once-per-session reload guard after a clean load. */
export function clearChunkReloadGuard(): void {
  if (typeof window === 'undefined') return
  try {
    sessionStorage.removeItem(RELOAD_FLAG)
  } catch {
    // ignore
  }
}
