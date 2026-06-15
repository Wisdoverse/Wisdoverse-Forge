/**
 * WebSocket Protocol Types
 *
 * Server-to-client and client-to-server message definitions.
 */

import type {
  ManagedAgent,
  TextTile,
  AgentGroup,
  WorkerReportPayload,
  TaskSummary,
  ParticipantSummary,
} from './agent.js'
import type { ClaudeEvent } from './events.js'

// ============================================================================
// WebSocket Messages
// ============================================================================

/** Permission option (number + label) */
export interface PermissionOption {
  number: string // "1", "2", "3"
  label: string // "Yes", "Yes, and always allow...", "No"
}

/** Server -> Client messages */
export type ServerMessage =
  | { type: 'event'; payload: ClaudeEvent }
  | { type: 'history'; payload: ClaudeEvent[] }
  | { type: 'connected'; payload: { agentId: string } }
  | { type: 'error'; payload: { message: string } }
  | { type: 'agents'; payload: ManagedAgent[] }
  | { type: 'agent_update'; payload: ManagedAgent }
  | {
      type: 'permission_prompt'
      payload: { agentId: string; tool: string; context: string; options: PermissionOption[] }
    }
  | { type: 'permission_resolved'; payload: { agentId: string } }
  | { type: 'text_tiles'; payload: TextTile[] }
  | { type: 'groups'; payload: AgentGroup[] }
  | { type: 'group_update'; payload: AgentGroup }
  | { type: 'worker_report'; payload: WorkerReportPayload }
  | { type: 'auth_success'; payload: { user: { id: string; email: string; username: string } } }
  | { type: 'auth_required' }
  | { type: 'auth_failed'; payload: { message: string } }
  | {
      type: 'collaborator_added'
      payload: {
        agentId: string
        collaborator: {
          id: string
          userId: string
          email: string
          username: string
          permission: 'view' | 'prompt' | 'manage'
          grantedBy: string | null
          grantedAt: number
        }
      }
    }
  | { type: 'collaborator_removed'; payload: { agentId: string; userId: string } }
  | {
      type: 'collaborator_updated'
      payload: { agentId: string; userId: string; permission: 'view' | 'prompt' | 'manage' }
    }
  | {
      type: 'ownership_transferred'
      payload: { agentId: string; oldOwnerId: string; newOwnerId: string }
    }
  | {
      type: 'output'
      payload: { agentId: string; lines: string[]; total: number; data?: string }
    }
  | { type: 'terminal_output'; payload: { agentId: string; data: string } }
  | { type: 'channel_agents'; payload: { channel: string; agents: ManagedAgent[] } }
  | {
      type: 'agent_health_changed'
      payload: {
        agentId: string
        health: 'ready' | 'degraded' | 'evicting' | 'rebuilding'
        reason: string
        message: string
      }
    }
  | { type: 'turn_invalidate'; payload: { agentId: string; timestamp: number } }
  | {
      type: 'orchestration:task_update'
      payload: { action: string; task: TaskSummary; eventId: string }
    }
  | {
      type: 'orchestration:participant_update'
      payload: { action: string; participant: ParticipantSummary; eventId: string }
    }
  | { type: 'pong' }
  | { type: 'server_draining'; payload: { reconnectMs: number } }
  | { type: 'voice_ready' }
  | {
      type: 'voice_transcript'
      payload: { transcript: string; isFinal: boolean; confidence?: number }
    }
  | { type: 'voice_utterance_end' }
  | { type: 'voice_error'; payload: { error: string } }
  // Admin-only CLI agent-image auto-updater toast. Delivered on the global
  // `broadcast.admin.cli_image` subject, which only owner/admin connections
  // subscribe to. Mirrors `CLI_IMAGE_UPDATED_EVENT` in the Rust core.
  // `update_available` and the version fields are claude's local-build mode
  // (no public registry image; versions come from npm, digests stay null).
  | {
      type: 'cli_image.updated'
      payload: {
        tool: string
        state: 'updated' | 'failed' | 'update_available'
        localDigest: string | null
        remoteDigest: string | null
        localVersion: string | null
        remoteVersion: string | null
        lastError: string | null
        eventId: string
        unix: number
      }
    }
  // Project git-clone status update. Broadcast on the project's scope subject
  // whenever a clone attempt changes state. Mirrors `CloneEvent::ws_frame` in
  // `rust/crates/api/src/domain/project_clone.rs`. `details` carries the
  // snake_case audit fields (`branch`, `head_sha`, `error_class`, `error_message`)
  // the worker emitted; `cloneStatus` is the denormalized project summary.
  | {
      type: 'project_clone:status_update'
      payload: {
        action: string
        eventId: string
        projectId: string
        cloneStatus: 'none' | 'queued' | 'cloning' | 'ready' | 'failed'
        details: Record<string, unknown>
      }
    }

/** Client -> Server messages */
export type ClientMessage =
  | {
      type: 'subscribe'
      payload?: {
        agentId?: string
        scope?: 'drafts' | 'project' | 'visible'
        projectId?: string
        restoreAgentId?: string
      }
    }
  | { type: 'get_history'; payload?: { limit?: number } }
  | { type: 'ping' }
  | { type: 'voice_start' }
  | { type: 'voice_stop' }
  | { type: 'permission_response'; payload: { agentId: string; response: string } }
  | { type: 'terminal_input'; payload: { agentId: string; keys: string[] } }
  | { type: 'terminal_attach'; payload: { agentId: string; cols: number; rows: number } }
  | { type: 'terminal_detach'; payload: { agentId: string } }
  | { type: 'terminal_data'; payload: { agentId: string; data: string } }
  | { type: 'terminal_resize'; payload: { agentId: string; cols: number; rows: number } }
  | { type: 'subscribe_channel'; payload: { channel: string; scope: 'active_all' } }
