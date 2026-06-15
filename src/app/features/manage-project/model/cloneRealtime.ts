import { useNavigationStore, type CloneStatusUpdate } from '@app/entities/navigation'
import type { CloneStatus, CloneSummary } from '@app/entities/project'

/**
 * The `project_clone:status_update` realtime frame the backend worker broadcasts
 * on a project's scope subject (see `CloneEvent::ws_frame` in
 * `rust/crates/api/src/domain/project_clone.rs`). The `details` object carries
 * the worker's snake_case audit fields; the top-level `cloneStatus` is the
 * denormalized project summary.
 */
export interface CloneStatusWsMessage {
  type: 'project_clone:status_update'
  payload?: unknown
  [key: string]: unknown
}

export const CLONE_STATUS_WS_TYPE = 'project_clone:status_update'

const CLONE_STATUSES: readonly CloneStatus[] = ['none', 'queued', 'cloning', 'ready', 'failed']

/**
 * Apply a clone-status realtime frame to the navigation tree. Pure parse +
 * delegate: the monotonic, idempotent state transition lives in the navigation
 * store's `applyCloneStatusUpdate`, so applying the same frame twice produces a
 * single state change. A malformed or unaddressed frame is silently ignored. A
 * missed frame self-heals on the next project list fetch (which carries
 * `cloneStatus`/`clone`).
 */
export function handleCloneStatusWsMessage(message: CloneStatusWsMessage): void {
  const update = parseCloneStatusUpdate(message)
  if (!update) return
  useNavigationStore.getState().applyCloneStatusUpdate(update)
}

/**
 * Decode the realtime frame into a `CloneStatusUpdate`, or `null` when the frame
 * is malformed (missing `projectId`/`cloneStatus`). Exported for unit testing the
 * parse/idempotency contract without a live socket.
 */
export function parseCloneStatusUpdate(message: CloneStatusWsMessage): CloneStatusUpdate | null {
  const payload = objectField(message.payload)
  if (!payload) return null

  const projectId = stringField(payload.projectId ?? payload.project_id)
  const cloneStatus = cloneStatusField(payload.cloneStatus ?? payload.clone_status)
  if (!projectId || !cloneStatus) return null

  return {
    projectId,
    cloneStatus,
    clone: cloneSummaryFromFrame(cloneStatus, objectField(payload.details)),
  }
}

/**
 * Build a `CloneSummary` from the frame's `details` audit object. The worker's
 * audit fields are snake_case (`branch`, `head_sha`, `error_class`,
 * `error_message`); `branch` maps to `resolvedBranch` to match the REST
 * `CloneSummary` shape. Returns `undefined` when `details` carries no usable
 * attempt info so the store keeps the existing summary instead of clobbering it.
 */
function cloneSummaryFromFrame(
  cloneStatus: CloneStatus,
  details: Record<string, unknown> | null
): CloneSummary | undefined {
  if (!details) return undefined
  const attempt = numberField(details.attempt)
  if (attempt === null) return undefined

  return {
    status: cloneStatus,
    attempt,
    updatedAt: stringField(details.updatedAt ?? details.updated_at) ?? new Date().toISOString(),
    resolvedBranch: stringField(
      details.resolvedBranch ?? details.resolved_branch ?? details.branch
    ),
    headSha: stringField(details.headSha ?? details.head_sha),
    errorClass: stringField(details.errorClass ?? details.error_class),
    errorMessage: stringField(details.errorMessage ?? details.error_message),
  }
}

function cloneStatusField(value: unknown): CloneStatus | null {
  return typeof value === 'string' && (CLONE_STATUSES as readonly string[]).includes(value)
    ? (value as CloneStatus)
    : null
}

function objectField(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null
}

function stringField(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value : undefined
}

function numberField(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && value.trim()) {
    const parsed = Number(value)
    if (Number.isFinite(parsed)) return parsed
  }
  return null
}
