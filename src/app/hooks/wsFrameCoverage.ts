import type { InboundServerMessage } from '@shared/types'

/**
 * Golden-fixture coverage for every frame type the browser decodes (#1327).
 * `satisfies Record<...>` makes a new union member without an entry a compile
 * error; tests/unit/app/useWsDispatch.test.ts checks this table against the
 * files in tests/fixtures/ws-protocol/ and dispatches every fixture.
 */
export type FrameFixture = { fixture: string } | { noFixture: string }

export const WS_FRAME_FIXTURES = {
  event: { fixture: 'event.json' },
  turn_invalidate: { fixture: 'turn_invalidate.json' },
  terminal_output: { fixture: 'terminal_output.json' },
  terminal_error: { fixture: 'terminal_error.json' },
  'cli_image.updated': { fixture: 'cli_image.updated.json' },
  'project_clone:status_update': { fixture: 'project_clone_status_update.json' },
  'orchestration:task_update': { fixture: 'orchestration_task_update.json' },
  'orchestration:participant_update': { fixture: 'orchestration_participant_update.json' },
  'credential:status_update': {
    noFixture: 'json! frame in rust/bins/server/src/main.rs publish_cli_credential_notifications',
  },
  'context_candidate.created': {
    noFixture:
      'json! frame in rust/crates/api/src/domain/context.rs ContextCandidateBroadcast::payload',
  },
  'context_candidate.approved': {
    noFixture:
      'json! frame in rust/crates/api/src/domain/context.rs ContextCandidateBroadcast::payload',
  },
  'context_candidate.rejected': {
    noFixture:
      'json! frame in rust/crates/api/src/domain/context.rs ContextCandidateBroadcast::payload',
  },
  'workflow:status': {
    noFixture:
      'realtime::Event in rust/crates/orchestrator/src/workflow/activities.rs emit_workflow_status',
  },
  'workflow:node_status': {
    noFixture:
      'realtime::Event in rust/crates/orchestrator/src/workflow/activities.rs emit_node_status',
  },
  'review.escalated': {
    noFixture: 'realtime::Event in rust/crates/orchestrator/src/review_escalation_reaper.rs',
  },
  'context_application.recorded': { noFixture: 'no Rust producer exists' },
  'context_feedback.submitted': { noFixture: 'no Rust producer exists' },
} as const satisfies Record<InboundServerMessage['type'], FrameFixture>

/** Gateway `{ ok: false, error }` control frames (no `type`), by fixture name. */
export const CONTROL_FRAME_FIXTURES = [
  'realtime_unavailable.json',
  'realtime_disconnected.json',
] as const
