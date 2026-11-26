import type {
  InboundServerMessage,
  OrchTaskPriority,
  OrchTaskState,
  RealtimeControlFrame,
  TaskSummary,
} from '@shared/types'

/**
 * One decoding step for every browser WebSocket frame (#1327): checks the frame
 * against the wire shape the Rust producers send (`shared/types/protocol.ts`)
 * and builds a typed message from small field readers. A frame that does not
 * match is reported as `unknown` instead of being half-applied.
 */
export type DecodedFrame =
  | { kind: 'message'; message: InboundServerMessage }
  | { kind: 'control'; frame: RealtimeControlFrame }
  | { kind: 'unrecognized'; key: string; detail?: string }

class MalformedFrame extends Error {}

function invalid(detail = ''): never {
  throw new MalformedFrame(detail)
}
function isRecord(v: unknown): v is Record<string, unknown> {
  return v !== null && typeof v === 'object' && !Array.isArray(v)
}
function obj(v: unknown): Record<string, unknown> {
  return isRecord(v) ? v : invalid()
}
function str(v: unknown): string {
  return typeof v === 'string' ? v : invalid()
}
/** Rust integer fields (`i16`/`i32`/`i64`/`u32`). */
function int(v: unknown): number {
  return typeof v === 'number' && Number.isSafeInteger(v) ? v : invalid()
}
function bool(v: unknown): boolean {
  return typeof v === 'boolean' ? v : invalid()
}
/** A closed vocabulary; the error names the field and the unrecognised value. */
function choice<V extends string>(v: unknown, values: readonly V[], field: string): V {
  return (
    values.find((value) => value === v) ??
    invalid(`${field} has unrecognised value ${JSON.stringify(v)?.slice(0, 64)}`)
  )
}
/** A field Rust always sends, possibly as `null` (`Option` without `skip_serializing_if`). */
function nullable<V>(v: unknown, read: (v: unknown) => V): V | null {
  return v === null ? null : read(v)
}
/** A field Rust omits when empty (`skip_serializing_if = "Option::is_none"`). */
function optional<V>(v: unknown, read: (v: unknown) => V): V | undefined {
  return v === undefined ? undefined : read(v)
}

const TASK_STATES: readonly OrchTaskState[] = [
  'backlog',
  'queued',
  'working',
  'blocked',
  'completed',
  'failed',
  'canceled',
]
const TASK_PRIORITIES: readonly OrchTaskPriority[] = ['low', 'normal', 'high', 'urgent']
const CONTEXT_CANDIDATE_TYPES = [
  'context_candidate.created',
  'context_candidate.approved',
  'context_candidate.rejected',
] as const
const UNPRODUCED_CONTEXT_TYPES = [
  'context_application.recorded',
  'context_feedback.submitted',
] as const

function taskSummary(v: unknown): TaskSummary {
  const t = obj(v)
  const params = obj(t.params)
  const counts = obj(t.contextCounts)
  return {
    id: str(t.id),
    groupId: nullable(t.groupId, str),
    state: choice(t.state, TASK_STATES, 'task.state'),
    method: str(t.method),
    params: { task: str(params.task), message: str(params.message) },
    priority: choice(t.priority, TASK_PRIORITIES, 'task.priority'),
    progress: int(t.progress),
    createdBy: str(t.createdBy),
    assignedTo: optional(t.assignedTo, str),
    assignedAgentName: optional(t.assignedAgentName, str),
    error: optional(t.error, str),
    result: t.result,
    blockedReason: optional(t.blockedReason, str),
    blockedHint: optional(t.blockedHint, str),
    // `Option<Value>`: any JSON the row stores; consumers must narrow it.
    blockedMetadata: t.blockedMetadata,
    createdAt: str(t.createdAt),
    updatedAt: str(t.updatedAt),
    rowVersion: optional(t.rowVersion, int),
    completedAt: optional(t.completedAt, str),
    selfFix: bool(t.selfFix),
    prNumber: optional(t.prNumber, int),
    prUrl: optional(t.prUrl, str),
    prHeadSha: optional(t.prHeadSha, str),
    reviewStatus: optional(t.reviewStatus, str),
    contextCounts: {
      appliedMemories: int(counts.appliedMemories),
      appliedSkills: int(counts.appliedSkills),
      total: int(counts.total),
    },
    attempt: int(t.attempt),
    leaseExpiresAt: optional(t.leaseExpiresAt, str),
    waitEstimate: optional(t.waitEstimate, (v) => {
      const w = obj(v)
      return {
        position: int(w.position),
        typicalSeconds: int(w.typicalSeconds),
        estimatedSeconds: int(w.estimatedSeconds),
      }
    }),
  }
}

function message(type: string, f: Record<string, unknown>): InboundServerMessage | null {
  const contextCandidate = CONTEXT_CANDIDATE_TYPES.find((t) => t === type)
  if (contextCandidate) {
    return {
      type: contextCandidate,
      candidateId: str(f.candidateId),
      itemKind: str(f.itemKind),
      state: str(f.state),
      scopeKind: str(f.scopeKind),
      scopeId: str(f.scopeId),
      timestamp: str(f.timestamp),
    }
  }
  const unproduced = UNPRODUCED_CONTEXT_TYPES.find((t) => t === type)
  if (unproduced) return { ...f, type: unproduced }

  switch (type) {
    case 'event':
      return {
        type,
        eventType: str(f.eventType),
        eventData: obj(f.eventData),
        agentId: str(f.agentId),
        orgId: str(f.orgId),
      }
    case 'turn_invalidate': {
      const p = obj(f.payload)
      return { type, payload: { agentId: str(p.agentId), timestamp: int(p.timestamp) } }
    }
    case 'terminal_output': {
      const p = obj(f.payload)
      return { type, payload: { agentId: str(p.agentId), data: str(p.data) } }
    }
    case 'terminal_error': {
      const p = obj(f.payload)
      return { type, payload: { agentId: str(p.agentId), message: str(p.message) } }
    }
    case 'orchestration:task_update': {
      const p = obj(f.payload)
      return {
        type,
        payload: { action: str(p.action), eventId: str(p.eventId), task: taskSummary(p.task) },
      }
    }
    case 'orchestration:participant_update': {
      const p = obj(f.payload)
      const participant = obj(p.participant)
      return {
        type,
        payload: {
          action: str(p.action),
          eventId: str(p.eventId),
          participant: {
            id: str(participant.id),
            agentId: str(participant.agentId),
            name: str(participant.name),
            status: choice(
              participant.status,
              ['online', 'busy', 'offline'] as const,
              'participant.status'
            ),
          },
        },
      }
    }
    case 'cli_image.updated': {
      const p = obj(f.payload)
      return {
        type,
        payload: {
          tool: str(p.tool),
          state: choice(p.state, ['updated', 'failed', 'update_available'] as const, 'state'),
          localDigest: nullable(p.localDigest, str),
          remoteDigest: nullable(p.remoteDigest, str),
          localVersion: nullable(p.localVersion, str),
          remoteVersion: nullable(p.remoteVersion, str),
          lastError: nullable(p.lastError, str),
          eventId: str(p.eventId),
          unix: int(p.unix),
        },
      }
    }
    case 'project_clone:status_update': {
      const p = obj(f.payload)
      return {
        type,
        payload: {
          action: str(p.action),
          eventId: str(p.eventId),
          projectId: str(p.projectId),
          cloneStatus: choice(
            p.cloneStatus,
            ['none', 'queued', 'cloning', 'ready', 'failed'] as const,
            'cloneStatus'
          ),
          details: obj(p.details),
        },
      }
    }
    case 'credential:status_update': {
      const p = obj(f.payload)
      const c = obj(p.credential)
      return {
        type,
        payload: {
          action: str(p.action),
          eventId: str(p.eventId),
          credential: {
            ownerUserId: str(c.ownerUserId),
            cliTool: str(c.cliTool),
            status: str(c.status),
            reason: str(c.reason),
            revokedAt: str(c.revokedAt),
          },
        },
      }
    }
    case 'workflow:status': {
      const p = obj(f.payload)
      return {
        type,
        orgId: str(f.orgId),
        payload: { workflowId: str(p.workflowId), status: str(p.status) },
      }
    }
    case 'workflow:node_status': {
      const p = obj(f.payload)
      return {
        type,
        orgId: str(f.orgId),
        payload: {
          nodeId: str(p.nodeId),
          nodeName: str(p.nodeName),
          status: str(p.status),
          detail: nullable(p.detail, str),
        },
      }
    }
    case 'review.escalated': {
      const p = obj(f.payload)
      return {
        type,
        orgId: str(f.orgId),
        payload: {
          reviewId: str(p.reviewId),
          taskId: str(p.taskId),
          dueAt: nullable(p.dueAt, str),
          overdueSecs: nullable(p.overdueSecs, int),
        },
      }
    }
    default:
      return null
  }
}

export function decodeServerMessage(data: unknown): DecodedFrame {
  if (!isRecord(data)) return { kind: 'unrecognized', key: '(not a JSON object)' }
  const type = data.type
  if (type === undefined) {
    return data.ok === false && typeof data.error === 'string'
      ? { kind: 'control', frame: { ok: false, error: data.error } }
      : { kind: 'unrecognized', key: '(no type)' }
  }
  if (typeof type !== 'string') return { kind: 'unrecognized', key: '(non-string type)' }
  try {
    const decoded = message(type, data)
    return decoded ? { kind: 'message', message: decoded } : { kind: 'unrecognized', key: type }
  } catch (err) {
    if (err instanceof MalformedFrame) {
      return { kind: 'unrecognized', key: `${type} (malformed)`, detail: err.message || undefined }
    }
    throw err
  }
}
