import type { SelfFixRepositorySetup } from '@shared/types/self-fix'
import { authFetch } from './authFetch'

export type RepositorySetupFailure =
  | 'forbidden'
  | 'unauthenticated'
  | 'not-configured'
  | 'permissions'
  | 'unavailable'
  | 'access'
  | 'empty-repository'
  | 'unsupported'
  | 'invalid-response'
  | 'timeout'
  | 'unreachable'
  | 'cancelled'
  | 'failed'

/** Safe, bounded reasons only: never retain an upstream body, URL or credential. */
export class RepositorySetupError extends Error {
  constructor(public readonly reason: RepositorySetupFailure) {
    super(`Repository connection check: ${reason}`)
    this.name = 'RepositorySetupError'
  }
}

export const REPOSITORY_CHECK_TIMEOUT_MS = 30_000

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function readSnapshot(payload: unknown): SelfFixRepositorySetup {
  if (!isRecord(payload) || payload.ok !== true || !isRecord(payload.data)) {
    throw new RepositorySetupError('invalid-response')
  }
  const data = payload.data
  if (
    typeof data.repository !== 'string' ||
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(data.repository) ||
    typeof data.defaultBranch !== 'string' ||
    !data.defaultBranch.trim() ||
    typeof data.baseSha !== 'string' ||
    !data.baseSha.trim() ||
    typeof data.contentsWrite !== 'boolean' ||
    typeof data.pullRequestsWrite !== 'boolean' ||
    typeof data.checksRead !== 'boolean' ||
    typeof data.squashMergeAllowed !== 'boolean'
  ) {
    throw new RepositorySetupError('invalid-response')
  }
  // Copy only the public contract. Extra fields (including secrets) cannot
  // enter component state even if an incompatible server includes them.
  return {
    repository: data.repository,
    defaultBranch: data.defaultBranch,
    baseSha: data.baseSha,
    contentsWrite: data.contentsWrite,
    pullRequestsWrite: data.pullRequestsWrite,
    checksRead: data.checksRead,
    squashMergeAllowed: data.squashMergeAllowed,
  }
}

function backendFailure(payload: unknown): RepositorySetupFailure {
  const code = isRecord(payload) && isRecord(payload.error) ? payload.error.code : null
  switch (code) {
    case 'errors.self_fix.github_not_configured':
      return 'not-configured'
    case 'errors.self_fix.repository_permissions':
      return 'permissions'
    case 'errors.self_fix.repository_unavailable':
      return 'unavailable'
    case 'errors.self_fix.repository_access':
      return 'access'
    case 'errors.self_fix.repository_base_unavailable':
      return 'empty-repository'
    default:
      return 'failed'
  }
}

/** A deliberate, read-only check with a deadline and caller cancellation. */
export async function getSelfFixRepository(signal?: AbortSignal): Promise<SelfFixRepositorySetup> {
  if (signal?.aborted) throw new RepositorySetupError('cancelled')
  const controller = new AbortController()
  let cancel = () => undefined
  let timer: ReturnType<typeof setTimeout> | undefined
  const interrupted = new Promise<never>((_resolve, reject) => {
    cancel = () => {
      controller.abort()
      reject(new RepositorySetupError('cancelled'))
    }
    timer = setTimeout(() => {
      controller.abort()
      reject(new RepositorySetupError('timeout'))
    }, REPOSITORY_CHECK_TIMEOUT_MS)
  })
  signal?.addEventListener('abort', cancel, { once: true })
  try {
    const response = await Promise.race([
      authFetch('/api/v1/self-fix/repository', {
        method: 'GET',
        cache: 'no-store',
        headers: { Accept: 'application/json' },
        signal: controller.signal,
      }),
      interrupted,
    ])
    if (response.status === 401) throw new RepositorySetupError('unauthenticated')
    if (response.status === 403) throw new RepositorySetupError('forbidden')
    if (response.status === 404) throw new RepositorySetupError('unsupported')
    if (!response.ok) {
      if (response.status !== 400) throw new RepositorySetupError('failed')
      const payload: unknown = await Promise.race([response.json().catch(() => null), interrupted])
      throw new RepositorySetupError(backendFailure(payload))
    }
    const payload: unknown = await Promise.race([response.json(), interrupted])
    return readSnapshot(payload)
  } catch (error) {
    if (error instanceof RepositorySetupError) throw error
    if (error instanceof SyntaxError) throw new RepositorySetupError('invalid-response')
    throw new RepositorySetupError('unreachable')
  } finally {
    clearTimeout(timer)
    signal?.removeEventListener('abort', cancel)
    controller.abort()
  }
}
