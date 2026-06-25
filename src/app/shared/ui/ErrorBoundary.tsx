import { Component, type ErrorInfo, type ReactNode } from 'react'
import { clearChunkReloadGuard, recoverFromChunkError } from '@app/shared/lib/chunkError'
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

  componentDidMount(): void {
    // A clean mount means the previous load (or reload) succeeded — reset the
    // once-per-session reload guard so a future, different chunk error can also
    // recover. Only clear on the no-error path so an error-on-mount cannot
    // re-arm a reload loop.
    if (!this.state.error) clearChunkReloadGuard()
  }

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
    window.location.reload()
  }

  render(): ReactNode {
    if (this.state.error) {
      return <ErrorFallback onReload={this.handleReload} />
    }
    return this.props.children
  }
}
