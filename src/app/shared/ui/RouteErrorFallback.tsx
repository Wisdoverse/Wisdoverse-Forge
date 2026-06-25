import { useEffect } from 'react'
import type { ErrorComponentProps } from '@tanstack/react-router'
import { clearChunkReloadGuard, recoverFromChunkError } from '@app/shared/lib/chunkError'
import { ErrorFallback } from './ErrorFallback'

/**
 * F069: the router's `defaultErrorComponent`. A loader/render throw in any route
 * — most importantly a failed lazy-chunk import for a code-split route after a
 * deploy — renders this in-shell recovery UI instead of bubbling to a blank
 * screen. Stale-hash chunk 404s auto-reload once (session-guarded).
 */
export function RouteErrorFallback({ error }: ErrorComponentProps) {
  useEffect(() => {
    recoverFromChunkError(error)
  }, [error])

  const handleReload = (): void => {
    clearChunkReloadGuard()
    window.location.reload()
  }

  return <ErrorFallback onReload={handleReload} />
}
