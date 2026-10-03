import type {
  MaintenanceObservation,
  MaintenanceRequestInput,
  MaintenanceSubmission,
  MaintenanceTrace,
} from '@shared/types/maintenance'
import { authFetch } from '@app/shared/api/authFetch'

export type MaintenanceFailure =
  | 'unauthenticated'
  | 'forbidden'
  | 'missing'
  | 'destination'
  | 'source-closed'
  | 'source-base'
  | 'source-unavailable'
  | 'setup'
  | 'invalid-request'
  | 'invalid-response'
  | 'timeout'
  | 'cancelled'
  | 'unreachable'
  | 'failed'
  | 'conflict'
  | 'invalid-delivery'
  | 'delivery-unavailable'

export class MaintenanceError extends Error {
  constructor(public readonly reason: MaintenanceFailure) {
    super(`Maintenance request: ${reason}`)
    this.name = 'MaintenanceError'
  }
}

export const MAINTENANCE_TIMEOUT_MS = 30_000
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const SHA = /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/i
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/

function invalid(): never {
  throw new MaintenanceError('invalid-response')
}
function record(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : invalid()
}
function string(value: unknown, pattern?: RegExp): string {
  return typeof value === 'string' && value.trim() && (!pattern || pattern.test(value))
    ? value
    : invalid()
}
function nullableString(value: unknown, pattern?: RegExp): string | null {
  return value === null ? null : string(value, pattern)
}
function boolean(value: unknown): boolean {
  return typeof value === 'boolean' ? value : invalid()
}
function integer(value: unknown, minimum = 0): number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum
    ? value
    : invalid()
}
function timestamp(value: unknown): string {
  const result = string(value)
  return Number.isFinite(Date.parse(result)) ? result : invalid()
}
function prUrl(value: unknown, repository: string): string | null {
  if (value === null) return null
  const url = string(value)
  const prefix = `https://github.com/${repository}/pull/`
  return url.toLowerCase().startsWith(prefix.toLowerCase()) &&
    /^[1-9]\d*$/.test(url.slice(prefix.length))
    ? `${prefix}${url.slice(prefix.length)}`
    : invalid()
}
function observation(value: unknown, repository: string): MaintenanceObservation {
  const data = record(value)
  const status = data.status
  if (
    status !== 'observed' &&
    status !== 'unavailable' &&
    status !== 'not_applicable' &&
    status !== 'not_created'
  )
    invalid()
  const checkedAt = data.checkedAt === null ? null : timestamp(data.checkedAt)
  const headChanged = boolean(data.headChanged)
  if (status !== 'observed') {
    if (data.snapshot !== null || headChanged) invalid()
    return { status, checkedAt, snapshot: null, headChanged }
  }
  if (checkedAt === null) invalid()
  const snapshot = record(data.snapshot)
  const number = integer(snapshot.number, 1)
  const state = snapshot.state
  if (state !== 'open' && state !== 'closed' && state !== 'merged') invalid()
  const url = prUrl(snapshot.url, repository)
  if (url !== `https://github.com/${repository}/pull/${number}`) invalid()
  return {
    status,
    checkedAt,
    headChanged,
    snapshot: {
      number,
      url,
      state,
      headSha: string(snapshot.headSha, SHA),
      baseBranch: string(snapshot.baseBranch),
    },
  }
}

function submission(value: unknown): MaintenanceSubmission {
  const data = record(value)
  return {
    requestId: string(data.requestId, UUID),
    taskId: string(data.taskId, UUID),
    reused: boolean(data.reused),
  }
}
function trace(value: unknown, taskId: string): MaintenanceTrace | null {
  if (value === null) return null
  const data = record(value)
  if (string(data.taskId, UUID) !== taskId) invalid()
  const repository = string(data.repository, REPOSITORY)
  const source = record(data.source)
  const kind = source.kind
  if (kind !== 'request' && kind !== 'pull_request') invalid()
  const reference = string(
    source.reference,
    kind === 'request' ? /^[a-z0-9][a-z0-9._:-]{0,127}$/ : /^[1-9]\d*$/
  )
  const sourceUrl = prUrl(source.url, repository)
  const submittedHeadSha = nullableString(source.submittedHeadSha, SHA)
  if (kind === 'request' && (sourceUrl !== null || submittedHeadSha !== null)) invalid()
  if (
    kind === 'pull_request' &&
    (sourceUrl !== `https://github.com/${repository}/pull/${reference}` ||
      submittedHeadSha === null)
  )
    invalid()
  if (!Array.isArray(data.executions)) invalid()
  return {
    requestId: string(data.requestId, UUID),
    taskId,
    taskState: string(data.taskState),
    attempt: integer(data.attempt),
    repository,
    defaultBranch: string(data.defaultBranch),
    startingSha: string(data.startingSha, SHA),
    createdAt: timestamp(data.createdAt),
    source: { kind, reference, url: sourceUrl, submittedHeadSha },
    executions: data.executions.map((value) => {
      const run = record(value)
      return {
        runId: string(run.runId, UUID),
        agentId: string(run.agentId, UUID),
        state: string(run.state),
        startedAt: timestamp(run.startedAt),
        finishedAt: run.finishedAt === null ? null : timestamp(run.finishedAt),
      }
    }),
    rebuildBaseSha: nullableString(data.rebuildBaseSha, SHA),
    recordedPrHeadSha: nullableString(data.recordedPrHeadSha, SHA),
    producedPrUrl: prUrl(data.producedPrUrl, repository),
    sourceState: observation(data.sourceState, repository),
    producedState: observation(data.producedState, repository),
  }
}

function backendFailure(payload: unknown): MaintenanceFailure {
  const error = record(payload).error
  if (typeof error !== 'object' || error === null) return 'failed'
  const code = (error as Record<string, unknown>).code
  switch (code) {
    case 'errors.maintenance.delivery_invalid':
      return 'invalid-delivery'
    case 'errors.maintenance.delivery_unavailable':
      return 'delivery-unavailable'
    case 'errors.maintenance.invalid_request':
      return 'invalid-request'
    case 'errors.maintenance.destination_unavailable':
      return 'destination'
    case 'errors.maintenance.source_closed':
      return 'source-closed'
    case 'errors.maintenance.source_base':
      return 'source-base'
    case 'errors.maintenance.source_unavailable':
      return 'source-unavailable'
    default:
      return typeof code === 'string' && code.startsWith('errors.self_fix.') ? 'setup' : 'failed'
  }
}

/** Bound the entire exchange, including the body. Never keep provider error text. */
async function exchange<T>(
  path: string,
  init: RequestInit,
  read: (data: unknown) => T,
  signal?: AbortSignal
): Promise<T> {
  if (signal?.aborted) throw new MaintenanceError('cancelled')
  const controller = new AbortController()
  let timer: ReturnType<typeof setTimeout> | undefined
  let cancel = () => undefined
  let interruption: 'cancelled' | 'timeout' | null = null
  const interrupted = new Promise<never>((_resolve, reject) => {
    cancel = () => {
      interruption = 'cancelled'
      controller.abort()
      reject(new MaintenanceError('cancelled'))
    }
    timer = setTimeout(() => {
      interruption = 'timeout'
      controller.abort()
      reject(new MaintenanceError('timeout'))
    }, MAINTENANCE_TIMEOUT_MS)
  })
  signal?.addEventListener('abort', cancel, { once: true })
  try {
    const response = await Promise.race([
      authFetch(path, {
        ...init,
        cache: 'no-store',
        headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
        signal: controller.signal,
      }),
      interrupted,
    ])
    if (response.status === 401) throw new MaintenanceError('unauthenticated')
    if (response.status === 403) throw new MaintenanceError('forbidden')
    if (response.status === 404) throw new MaintenanceError('missing')
    if (response.status === 409) throw new MaintenanceError('conflict')
    if (!response.ok) {
      if (response.status !== 400) throw new MaintenanceError('failed')
      const payload: unknown = await Promise.race([response.json().catch(() => null), interrupted])
      throw new MaintenanceError(backendFailure(payload))
    }
    const payload = record(await Promise.race([response.json(), interrupted]))
    if (payload.ok !== true) invalid()
    return read(payload.data)
  } catch (error) {
    if (interruption) throw new MaintenanceError(interruption)
    if (error instanceof MaintenanceError) throw error
    throw new MaintenanceError(error instanceof SyntaxError ? 'invalid-response' : 'unreachable')
  } finally {
    clearTimeout(timer)
    signal?.removeEventListener('abort', cancel)
    controller.abort()
  }
}

export function submitMaintenanceRequest(input: MaintenanceRequestInput, signal?: AbortSignal) {
  return exchange(
    '/api/v1/self-fix/requests',
    { method: 'POST', body: JSON.stringify(input) },
    submission,
    signal
  )
}
export function getMaintenanceTrace(taskId: string, signal?: AbortSignal) {
  if (!UUID.test(taskId)) return Promise.reject(new MaintenanceError('invalid-request'))
  return exchange(
    `/api/v1/self-fix/tasks/${taskId}/trace`,
    { method: 'GET' },
    (data) => trace(data, taskId),
    signal
  )
}

// Shared within this entity slice; UI consumers use its public index.
export { exchange, trace }
