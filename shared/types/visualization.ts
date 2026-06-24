/**
 * Visualization & Configuration Types
 *
 * 3D visualization state, utility types for tool inputs, and config.
 */

// ============================================================================
// Visualization State
// ============================================================================

/** Represents Claude's current activity state */
export type ClaudeState =
  | 'idle' // Waiting for user input
  | 'thinking' // Processing (between tools)
  | 'working' // Using a tool
  | 'finished' // Completed response

// ============================================================================
// Utility Types
// ============================================================================

/** Extract specific tool input types */
export interface BashToolInput {
  command: string
  description?: string
  timeout?: number
  run_in_background?: boolean
}

export interface WriteToolInput {
  file_path: string
  content: string
}

export interface EditToolInput {
  file_path: string
  old_string: string
  new_string: string
  replace_all?: boolean
}

export interface ReadToolInput {
  file_path: string
  offset?: number
  limit?: number
}

export interface TaskToolInput {
  description: string
  prompt: string
  subagent_type: string
}

// ============================================================================
// Configuration
// ============================================================================

export interface AgentForgeConfig {
  /** WebSocket server port */
  serverPort: number
  /** Path to events JSONL file */
  eventsFile: string
  /** Maximum events to keep in memory */
  maxEventsInMemory: number
  /** Enable debug logging */
  debug: boolean
}

export const DEFAULT_CONFIG: AgentForgeConfig = {
  serverPort: 4003,
  eventsFile: './data/events.jsonl',
  maxEventsInMemory: 1000,
  debug: false,
}
