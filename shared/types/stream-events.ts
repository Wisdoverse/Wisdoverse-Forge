/**
 * Canonical Stream Event Schema
 *
 * Normalized event types for SSE/NDJSON streaming.
 * These project FROM the raw hook events (ClaudeEvent) into a stable,
 * agent-friendly format suitable for CLI and programmatic consumption.
 *
 * Projection rules (raw → stream):
 *   pre_tool_use          → tool_start  (+ assistant_text if present)
 *   post_tool_use         → tool_finish
 *   stop / subagent_stop  → agent_idle  (+ response text if present)
 *   session_start         → agent_start
 *   session_end           → agent_end
 *   user_prompt_submit    → prompt_submitted
 *   notification          → notification (permission_prompt → permission_request)
 *   terminal_output       → (excluded from stream by default)
 *   pre_compact           → (excluded from stream by default)
 */

// ============================================================================
// Stream Event Types
// ============================================================================

/** All canonical stream event type strings */
export type StreamEventType =
  | 'assistant_text'
  | 'tool_start'
  | 'tool_finish'
  | 'agent_idle'
  | 'agent_start'
  | 'agent_end'
  | 'prompt_submitted'
  | 'permission_request'
  | 'notification'
  | 'error'

// ============================================================================
// Base Stream Event
// ============================================================================

export interface BaseStreamEvent {
  /** Canonical stream event type */
  type: StreamEventType
  /** Agent UUID (managed entity ID, not cliSessionId) */
  agentId: string
  /** ISO 8601 timestamp */
  ts: string
  /** Original event UUID (for cursor/dedup) */
  eventId: string
}

// ============================================================================
// Stream Events
// ============================================================================

/** Assistant text output (extracted from pre_tool_use.assistantText or stop.response) */
export interface AssistantTextEvent extends BaseStreamEvent {
  type: 'assistant_text'
  /** The assistant's text content */
  content: string
}

/** Tool invocation started (projected from pre_tool_use) */
export interface ToolStartEvent extends BaseStreamEvent {
  type: 'tool_start'
  /** Tool name (Read, Write, Bash, etc.) */
  tool: string
  /** Tool input parameters */
  input: Record<string, unknown>
  /** Tool use correlation ID */
  toolUseId: string
}

/** Tool invocation completed (projected from post_tool_use) */
export interface ToolFinishEvent extends BaseStreamEvent {
  type: 'tool_finish'
  /** Tool name */
  tool: string
  /** Tool output/response */
  output: Record<string, unknown>
  /** Tool use correlation ID (matches tool_start) */
  toolUseId: string
  /** Whether the tool succeeded */
  success: boolean
  /** Duration in milliseconds */
  durationMs?: number
}

/** Agent became idle — work completed (projected from stop/subagent_stop) */
export interface AgentIdleEvent extends BaseStreamEvent {
  type: 'agent_idle'
  /** Agent's final response text (if available from stop event) */
  response?: string
}

/** Agent session started (projected from session_start) */
export interface StreamAgentStartEvent extends BaseStreamEvent {
  type: 'agent_start'
  /** Start source */
  source: 'startup' | 'resume' | 'clear' | 'compact'
}

/** Agent session ended (projected from session_end) */
export interface StreamAgentEndEvent extends BaseStreamEvent {
  type: 'agent_end'
  /** End reason */
  reason: string
}

/** User prompt was submitted (projected from user_prompt_submit) */
export interface PromptSubmittedEvent extends BaseStreamEvent {
  type: 'prompt_submitted'
  /** The prompt text */
  prompt: string
}

/** Permission request requiring user response (projected from notification with permission_prompt) */
export interface PermissionRequestEvent extends BaseStreamEvent {
  type: 'permission_request'
  /** The permission prompt message */
  message: string
}

/** Generic notification (projected from notification, excluding permission_prompt) */
export interface NotificationStreamEvent extends BaseStreamEvent {
  type: 'notification'
  /** Notification message */
  message: string
  /** Notification subtype */
  notificationType?: string
}

/** Error event (synthesized on stream errors) */
export interface ErrorStreamEvent extends BaseStreamEvent {
  type: 'error'
  /** Error message */
  message: string
  /** Error code if available */
  code?: string
}

// ============================================================================
// Union Type
// ============================================================================

export type StreamEvent =
  | AssistantTextEvent
  | ToolStartEvent
  | ToolFinishEvent
  | AgentIdleEvent
  | StreamAgentStartEvent
  | StreamAgentEndEvent
  | PromptSubmittedEvent
  | PermissionRequestEvent
  | NotificationStreamEvent
  | ErrorStreamEvent

// ============================================================================
// SSE Cursor (for reconnection)
// ============================================================================

/**
 * Composite cursor for SSE resume.
 * Encodes (timestamp, eventId) as opaque base64 string.
 * Client sends this as Last-Event-ID header on reconnect.
 */
export interface StreamCursor {
  /** Unix timestamp in milliseconds */
  ts: number
  /** Event UUID */
  id: string
}

const cursorTextEncoder = new TextEncoder()
const cursorTextDecoder = new TextDecoder()

function encodeBase64Url(input: string): string {
  const binary = Array.from(cursorTextEncoder.encode(input), (byte) =>
    String.fromCharCode(byte)
  ).join('')

  return btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '')
}

function decodeBase64Url(encoded: string): string {
  const base64 = encoded.replaceAll('-', '+').replaceAll('_', '/')
  const padded = base64.padEnd(base64.length + ((4 - (base64.length % 4)) % 4), '=')
  const binary = atob(padded)
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0))

  return cursorTextDecoder.decode(bytes)
}

/** Encode cursor to opaque string for Last-Event-ID */
export function encodeCursor(cursor: StreamCursor): string {
  return encodeBase64Url(`${cursor.ts}:${cursor.id}`)
}

/** Decode opaque Last-Event-ID to cursor. Returns null if invalid. */
export function decodeCursor(encoded: string): StreamCursor | null {
  try {
    const decoded = decodeBase64Url(encoded)
    const colonIdx = decoded.indexOf(':')
    if (colonIdx === -1) return null
    const ts = Number(decoded.slice(0, colonIdx))
    const id = decoded.slice(colonIdx + 1)
    if (!Number.isFinite(ts) || !id) return null
    return { ts, id }
  } catch {
    return null
  }
}

// ============================================================================
// SSE Control Events (not data events — sent as SSE event type)
// ============================================================================

/** Sent when server buffer overflows. Includes cursor range for gap detection. */
export interface OverflowSignal {
  /** Total number of events dropped since connection start */
  dropped: number
  /** Cursor of the first dropped event (for targeted replay) */
  oldest: string
  /** Cursor of the most recently dropped event */
  newest: string
}

// ============================================================================
// Waiter Result
// ============================================================================

/** Returned by GET /api/v1/agents/{id}/wait */
export interface AgentWaitResult {
  /** Final agent status */
  status: string
  /** Agent's response text (from last stop event, if available) */
  response?: string
  /** Cursor for subsequent event queries */
  cursor?: string
  /** Whether the wait timed out */
  timedOut: boolean
}

// ============================================================================
// Stream Event Filter
// ============================================================================

/** Types that are excluded from stream by default */
export const STREAM_EXCLUDED_RAW_TYPES = new Set(['terminal_output', 'pre_compact'])

/** All valid stream event type strings for filter validation */
export const VALID_STREAM_TYPES = new Set<StreamEventType>([
  'assistant_text',
  'tool_start',
  'tool_finish',
  'agent_idle',
  'agent_start',
  'agent_end',
  'prompt_submitted',
  'permission_request',
  'notification',
  'error',
])
