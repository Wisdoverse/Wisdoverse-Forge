/**
 * WebSocket Protocol Types
 *
 * Server-to-client and client-to-server message definitions.
 */

import type { TaskSummary, ParticipantSummary } from './agent.js'

// ============================================================================
// WebSocket Messages
// ============================================================================

/**
 * Version of the browser <-> platform WebSocket protocol. Mirrors
 * `PROTOCOL_VERSION` in `rust/crates/core/src/ws_protocol.rs`; the
 * `check-protocol-contract.mjs` gate fails if the two diverge. Served on
 * `GET /me` as `protocolVersion`.
 */
export const PROTOCOL_VERSION = 1

/** Server -> Client messages */
// NOTE (MS-3): only variants with a live Rust producer remain. ~30 legacy
// TypeScript-server frames (agents, groups, collaborator_*, voice_*, auth_*,
// pong, …) were removed in PR-A because no Rust code ever emits them; see
// docs/architecture/ms3-ws-protocol-baseline.md. The wire baseline is pinned in
// tests/fixtures/ws-protocol/ and guarded by scripts/check-protocol-contract.mjs.
export type ServerMessage =
  // Flat activity event relayed from the Rust gateway (BroadcastMessage). The
  // event detail lives in `eventData` (a normalized superset of ClaudeEvent with
  // injected `type`/`orgId`/`sessionId`/`timestamp`/`id`), NOT a nested payload.
  | {
      type: 'event'
      eventType: string
      eventData: Record<string, unknown>
      agentId: string
      orgId: string
    }
  | { type: 'terminal_output'; payload: { agentId: string; data: string } }
  // Live gateway frame sent when a terminal attach/input fails. Mirrors
  // `terminal_error_frame` in rust/crates/api/src/domain/gateway.rs and the
  // tests/fixtures/ws-protocol/terminal_error.json golden fixture.
  | { type: 'terminal_error'; payload: { agentId: string; message: string } }
  | { type: 'turn_invalidate'; payload: { agentId: string; timestamp: number } }
  | {
      type: 'orchestration:task_update'
      payload: { action: string; task: TaskSummary; eventId: string }
    }
  | {
      type: 'orchestration:participant_update'
      payload: { action: string; participant: ParticipantSummary; eventId: string }
    }
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

/** Client -> Server messages. Only the terminal_* tags are parsed by the Rust
 * `handle_client_message` (ws.rs); the old subscribe/voice/ping/permission tags
 * were removed in MS-3 PR-A (never handled server-side). */
export type ClientMessage =
  | { type: 'terminal_input'; payload: { agentId: string; keys: string[] } }
  | { type: 'terminal_attach'; payload: { agentId: string; cols: number; rows: number } }
  | { type: 'terminal_detach'; payload: { agentId: string } }
  | { type: 'terminal_data'; payload: { agentId: string; data: string } }
  | { type: 'terminal_resize'; payload: { agentId: string; cols: number; rows: number } }

// ============================================================================
// REST: GET /api/v1/me
// ============================================================================

/**
 * Response body for `GET /api/v1/me`.
 *
 * `isAdmin` (camelCase) is the GLOBAL `users.is_admin` flag, looked up
 * server-side (the JWT does NOT carry it). The frontend gates the admin console
 * on this — the same platform-admin authority the backend `/admin/*` gate uses
 * (#881) — not the self-assignable per-org `role`. The snake_case `user_id` /
 * `org_id` / `role` fields preserve the legacy contract.
 */
export interface MeResponse {
  ok: boolean
  user_id: string
  org_id: string
  role: string
  isAdmin: boolean
  /** WS wire-contract version (MS-3). Optional for one release cycle (rolling deploys). */
  protocolVersion?: number
}
