# Turn API Reference

The Turn API exposes the chat-style projection for a managed Agent. Raw
runtime events are projected into chronological Turns with optional tool Steps.

Base URL: `http://localhost:4003` in local development.

## Authentication

All endpoints require a Bearer JWT token from `/api/v1/auth/login`:

```http
Authorization: Bearer <token>
```

The Rust API resolves tenant scope from the authenticated user and constrains
the queried Agent to that scope.

## Endpoints

### List Agent Turns

```http
GET /api/v1/agents/:agentId/turns
```

Returns a cursor-paginated turn list for an Agent. The first page is the newest
available projection in chronological order. Keep requesting with the returned
`cursor` while `hasMore` is true.

Query parameters:

| Parameter | Type    | Default | Description                          |
| --------- | ------- | ------- | ------------------------------------ |
| `cursor`  | string  | none    | Opaque cursor from previous response |
| `limit`   | integer | `50`    | Page size, clamped server-side       |

Example:

```bash
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:4003/api/v1/agents/550e8400-e29b-41d4-a716-446655440000/turns?limit=50"
```

Response:

```json
{
  "ok": true,
  "turns": [
    {
      "id": "evt_abc123",
      "sessionId": "cli_sess_001",
      "sequence": 1,
      "type": "user",
      "status": "complete",
      "prompt": "Read the README",
      "steps": [],
      "startedAt": 1709812800000,
      "completedAt": 1709812800000,
      "rawEventCount": 1
    }
  ],
  "cursor": "eyJzZXF1ZW5jZSI6NTB9",
  "hasMore": true,
  "totalTurnCount": 12,
  "lastEvent": {
    "timestamp": "2026-03-22T10:00:00Z",
    "id": "evt_abc123"
  }
}
```

## WebSocket Invalidation

The WebSocket gateway is `ws://localhost:4003/ws`. When persisted events change
the projected turn feed, the server emits:

```json
{
  "type": "turn_invalidate",
  "payload": {
    "agentId": "550e8400-e29b-41d4-a716-446655440000",
    "timestamp": 1709812800000
  }
}
```

Clients should refetch the first page for that Agent and reconcile local state.

## Notes

- The current Rust route is Agent-scoped: `/api/v1/agents/:agentId/turns`.
- There is no active HTTP step-content endpoint. Step preview and full content
  behavior is handled by the current event projection and frontend cache.
- `cliSessionId`, `BaseEvent.sessionId`, `session_start`, and `session_end`
  remain external CLI/hook protocol names and should not be renamed in payloads.

## Source of Truth

- Route: `rust/crates/api/src/routes/turns.rs`
- Projection types: `shared/turn-builder.ts`
- WebSocket message type: `shared/types/protocol.ts`
