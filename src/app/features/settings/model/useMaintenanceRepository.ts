import { useEffect, useRef, useState } from 'react'
import type { SelfFixRepositorySetup } from '@shared/types/self-fix'
import {
  getSelfFixRepository,
  RepositorySetupError,
  type RepositorySetupFailure,
} from '@app/shared/api/selfFix'

type RepositoryState =
  | { status: 'restricted' }
  | { status: 'loading' }
  | { status: 'error'; reason: RepositorySetupFailure }
  | { status: 'ready'; snapshot: SelfFixRepositorySetup; checkedAt: string }

/** Scope display state to the current identity, and discard every old request. */
export function useMaintenanceRepository(enabled: boolean, identity: string) {
  const [result, setResult] = useState<{ identity: string; state: RepositoryState }>({
    identity,
    state: { status: 'loading' },
  })
  const [refreshVersion, setRefreshVersion] = useState(0)
  const active = useRef<AbortController | null>(null)

  useEffect(() => {
    if (!enabled) {
      setResult({ identity, state: { status: 'restricted' } })
      return
    }
    const controller = new AbortController()
    active.current = controller
    setResult({ identity, state: { status: 'loading' } })
    void getSelfFixRepository(controller.signal).then(
      (snapshot) => {
        if (!controller.signal.aborted) {
          setResult({
            identity,
            state: { status: 'ready', snapshot, checkedAt: new Date().toISOString() },
          })
        }
      },
      (error: unknown) => {
        if (!controller.signal.aborted) {
          setResult({
            identity,
            state: {
              status: 'error',
              reason: error instanceof RepositorySetupError ? error.reason : 'failed',
            },
          })
        }
      }
    )
    return () => {
      controller.abort()
      if (active.current === controller) active.current = null
    }
  }, [enabled, identity, refreshVersion])

  function refresh(): void {
    if (!enabled) return
    active.current?.abort()
    setResult({ identity, state: { status: 'loading' } })
    setRefreshVersion((version) => version + 1)
  }

  // Invalidate synchronously on logout/role/identity changes, before effects
  // run, so another account never gets one frame of the previous snapshot.
  const state: RepositoryState = !enabled
    ? { status: 'restricted' }
    : result.identity === identity
      ? result.state
      : { status: 'loading' }
  return { state, refresh }
}
