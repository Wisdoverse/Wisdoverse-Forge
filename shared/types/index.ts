/**
 * Barrel re-export for shared types.
 *
 * Import from 'shared/types/events', 'shared/types/protocol', etc. for new code.
 * This barrel ensures everything is available from 'shared/types' for backward compatibility.
 */

export * from './events.js'
export * from './protocol.js'
export * from './agent.js'
export * from './context.js'
export * from './runtime-capability.js'
export * from './tools.js'
export * from './visualization.js'
export {
  type StreamEventType,
  type BaseStreamEvent,
  type AssistantTextEvent,
  type ToolStartEvent,
  type ToolFinishEvent,
  type AgentIdleEvent,
  type StreamAgentStartEvent,
  type StreamAgentEndEvent,
  type PromptSubmittedEvent,
  type PermissionRequestEvent as StreamPermissionRequestEvent,
  type NotificationStreamEvent,
  type ErrorStreamEvent,
  type StreamEvent,
  type StreamCursor,
  encodeCursor,
  decodeCursor,
  type OverflowSignal,
  type AgentWaitResult,
  STREAM_EXCLUDED_RAW_TYPES,
  VALID_STREAM_TYPES,
} from './stream-events.js'
