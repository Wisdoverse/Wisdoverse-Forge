import { Component, type ErrorInfo, type ReactNode } from 'react'
import {
  clearChunkReloadGuard,
  recoverFromChunkError,
  reloadPage,
} from '@app/shared/lib/chunkError'
import { ErrorFallback } from './ErrorFallback'

type ErrorBoundaryProps = {
  children: ReactNode
}

type ErrorBoundaryState = {
  error: Error | null
}

/**
 * F069: top-level React error boundary. Without one, a failed lazy-chunk import
 * (common right after a deploy) or any unexpected render throw propagates to the
 * React root and renders a blank white screen — a hard outage from the user's
 * perspective, with no client-side capture. This renders a recovery UI instead
 * and auto-reloads once for stale-hash chunk 404s.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error }
  }

  // NB: the once-per-session reload guard is deliberately NOT cleared on mount.
  // A lazy chunk needed during the FIRST render rejects asynchronously, AFTER
  // React has already committed the pending UI and called `componentDidMount`
  // with `state.error` still null; clearing here would drop the guard set by the
  // previous recovery reload and loop forever on a persistently-broken chunk.
  // The guard lives for the browser session (so we auto-reload at most once) and
  // is re-armed only on an explicit user reload (see `handleReload`).

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // Stale-hash chunk 404 right after a deploy → reload once to fetch the new
    // index + chunk hashes. recoverFromChunkError is session-guarded so a
    // genuinely-broken chunk falls through to the recovery UI instead of looping.
    if (recoverFromChunkError(error)) return
    // Error-reporting hook. Kept console-only until a telemetry sink is wired.
    console.error('Unhandled UI error', error, info.componentStack)
  }

  private handleReload = (): void => {
    clearChunkReloadGuard()
    reloadPage()
  }

  render(): ReactNode {
    if (this.state.error) {
      return <ErrorFallback onReload={this.handleReload} />
    }
    return this.props.children
  }
}
