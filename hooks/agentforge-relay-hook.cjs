#!/usr/bin/env node
// Thin relay hook — reads Claude Code hook JSON from stdin, transforms it,
// and sends to the Go sidecar via Unix Domain Socket (length-prefixed framing).
// Exits immediately after sending (~10ms lifetime, 1 PID).

'use strict'

const net = require('node:net')
const fs = require('node:fs')
const { randomUUID } = require('node:crypto')

const SOCKET_PATH = process.env.AGENTFORGE_RELAY_SOCKET || '/tmp/agentforge-relay.sock'
const MAX_RESPONSE_CHARS = 65536 // 64K characters max response text

// =============================================================================
// Event transformation (ported from agentforge-hook.sh)
// =============================================================================

const EVENT_TYPE_MAP = {
  // Claude Code events
  PreToolUse: 'pre_tool_use',
  PermissionRequest: 'permission_request',
  PostToolUse: 'post_tool_use',
  Stop: 'stop',
  SubagentStop: 'subagent_stop',
  SessionStart: 'session_start',
  SessionEnd: 'session_end',
  UserPromptSubmit: 'user_prompt_submit',
  Notification: 'notification',
  PreCompact: 'pre_compact',
  // Gemini CLI events (mapped to same internal types)
  BeforeTool: 'pre_tool_use',
  AfterTool: 'post_tool_use',
  AfterAgent: 'stop',
  BeforeAgent: 'user_prompt_submit',
  PreCompress: 'pre_compact',
}

// Maps hook_event_name to the CLI tool that produced it
const CLAUDE_EVENTS = new Set([
  'PreToolUse',
  'PostToolUse',
  'Stop',
  'SubagentStop',
  'SessionStart',
  'SessionEnd',
  'UserPromptSubmit',
  'Notification',
  'PreCompact',
])
const GEMINI_EVENTS = new Set([
  'BeforeTool',
  'AfterTool',
  'AfterAgent',
  'BeforeAgent',
  'PreCompress',
])
const CODEX_EVENTS = new Set([
  'PreToolUse',
  'PermissionRequest',
  'PostToolUse',
  'SessionStart',
  'Stop',
  'UserPromptSubmit',
])

function detectSourceHookType(hookEventName) {
  // Environment variable override is the most reliable source
  const envOverride = process.env.AGENTFORGE_CLI_TOOL
  if (envOverride) return envOverride

  if (CLAUDE_EVENTS.has(hookEventName)) return 'claude'
  if (CODEX_EVENTS.has(hookEventName)) return 'codex'
  if (GEMINI_EVENTS.has(hookEventName)) return 'gemini'
  return 'unknown'
}

function truncateJson(value, maxBytes) {
  if (value === undefined || value === null) return {}
  const serialized = JSON.stringify(value)
  if (serialized.length > (maxBytes || 131072)) {
    return { _truncated: true, _original_size: serialized.length }
  }
  return value
}

// =============================================================================
// Transcript extraction — reads Claude Code JSONL transcript to get response
// =============================================================================

function readTranscriptResponse(transcriptPath) {
  const content = fs.readFileSync(transcriptPath, 'utf8')
  const lines = content.trim().split('\n')

  // Scan from the end to find the last assistant message
  for (let i = lines.length - 1; i >= 0; i--) {
    let entry
    try {
      entry = JSON.parse(lines[i])
    } catch {
      continue
    }
    if (entry.type !== 'assistant') continue

    const blocks = entry.message?.content
    if (!Array.isArray(blocks)) continue

    const text = blocks
      .filter((b) => b.type === 'text' && b.text)
      .map((b) => b.text)
      .join('\n')
      .trim()

    if (text) {
      return text.length > MAX_RESPONSE_CHARS
        ? text.slice(0, MAX_RESPONSE_CHARS) + '\n…(truncated)'
        : text
    }
  }
  return ''
}

function extractLastAssistantResponse(transcriptPath) {
  if (!transcriptPath) return ''
  // The Stop hook may fire before the transcript's final assistant entry is
  // flushed to disk. Retry a few times with small delays to handle the race.
  // If still empty after retries, the FeedManager will fall back to tracked
  // assistantText from the most recent pre_tool_use event.
  for (let attempt = 0; attempt < 5; attempt++) {
    try {
      const text = readTranscriptResponse(transcriptPath)
      if (text) return text
    } catch (err) {
      process.stderr.write(
        `agentforge-relay-hook: transcript extraction failed (attempt ${attempt + 1}/5) for ${transcriptPath}: ${err?.message ?? err}\n`
      )
      break
    }
    // Brief synchronous sleep (50ms) — acceptable in a short-lived hook process
    try {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 50)
    } catch {
      /* Node < 22 */
    }
  }
  return ''
}

function extractAssistantText(transcriptPath, toolUseId) {
  if (!transcriptPath || !toolUseId) return ''
  try {
    const content = fs.readFileSync(transcriptPath, 'utf8')
    const lines = content.trim().split('\n')

    // Find the last assistant message containing this tool_use_id
    for (let i = lines.length - 1; i >= 0; i--) {
      let entry
      try {
        entry = JSON.parse(lines[i])
      } catch {
        continue
      }
      if (entry.type !== 'assistant') continue

      const blocks = entry.message?.content
      if (!Array.isArray(blocks)) continue

      const hasToolUse = blocks.some((b) => b.type === 'tool_use' && b.id === toolUseId)
      if (!hasToolUse) continue

      // Collect text blocks that appear before this tool_use block
      const textParts = []
      for (const b of blocks) {
        if (b.type === 'tool_use' && b.id === toolUseId) break
        if (b.type === 'text' && b.text) textParts.push(b.text)
      }
      const text = textParts.join('\n').trim()
      if (text) {
        return text.length > MAX_RESPONSE_CHARS
          ? text.slice(0, MAX_RESPONSE_CHARS) + '\n…(truncated)'
          : text
      }
      return ''
    }
  } catch (err) {
    process.stderr.write(
      `agentforge-relay-hook: assistantText extraction failed for ${transcriptPath}: ${err?.message ?? err}\n`
    )
  }
  return ''
}

function transformEvent(input) {
  const hookName = input.hook_event_name || 'unknown'
  const eventType = EVENT_TYPE_MAP[hookName] || 'unknown'
  const sessionId = input.session_id || 'unknown'
  const cwd = input.cwd || ''
  const runtimeId = process.env.AGENTFORGE_AGENT_ID || ''

  const now = Date.now()
  const eventId = randomUUID()

  const base = {
    schemaVersion: 1,
    id: eventId,
    timestamp: now,
    type: eventType,
    sessionId,
    cwd,
    runtimeId,
    cliTool: detectSourceHookType(hookName),
    sourceType: 'native-hook',
    sourceHookType: detectSourceHookType(hookName),
  }

  switch (eventType) {
    case 'pre_tool_use':
      return {
        ...base,
        tool: input.tool_name || 'unknown',
        toolInput: truncateJson(input.tool_input),
        toolUseId: input.tool_use_id || '',
        assistantText: input.tool_use_id
          ? extractAssistantText(input.transcript_path, input.tool_use_id)
          : '',
      }

    case 'post_tool_use':
      return {
        ...base,
        tool: input.tool_name || 'unknown',
        toolInput: truncateJson(input.tool_input),
        toolResponse: truncateJson(input.tool_response),
        toolUseId: input.tool_use_id || '',
        success: input.tool_response?.success !== undefined ? input.tool_response.success : true,
      }

    case 'permission_request':
      return {
        ...base,
        tool: input.tool_name || 'unknown',
        toolInput: truncateJson(input.tool_input),
        description: input.tool_input?.description || '',
      }

    case 'stop':
    case 'subagent_stop':
      return {
        ...base,
        stopHookActive: input.stop_hook_active || false,
        response: input.prompt_response || extractLastAssistantResponse(input.transcript_path),
      }

    case 'session_start':
      return {
        ...base,
        source: input.source || 'startup',
      }

    case 'session_end':
      return {
        ...base,
        reason: input.reason || 'other',
      }

    case 'user_prompt_submit':
      return {
        ...base,
        prompt: input.prompt || '',
      }

    case 'notification':
      return {
        ...base,
        message: input.message || '',
        notificationType: input.notification_type || 'unknown',
      }

    case 'pre_compact':
      return {
        ...base,
        trigger: input.trigger || 'manual',
        customInstructions: input.custom_instructions || '',
      }

    default:
      return {
        ...base,
        raw: input,
      }
  }
}

// =============================================================================
// Send via UDS (length-prefixed frame)
// =============================================================================

function sendViaUds(eventJson) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(SOCKET_PATH, () => {
      const payload = Buffer.from(eventJson, 'utf8')
      const header = Buffer.alloc(4)
      header.writeUInt32BE(payload.length, 0)
      socket.write(header)
      socket.write(payload, () => {
        socket.end()
        resolve()
      })
    })

    socket.on('error', reject)
    socket.setTimeout(2000, () => {
      socket.destroy(new Error('UDS connect timeout'))
    })
  })
}

// =============================================================================
// Main
// =============================================================================

async function main() {
  const chunks = []
  for await (const chunk of process.stdin) {
    chunks.push(chunk)
  }
  const rawInput = Buffer.concat(chunks).toString('utf8').trim()
  if (!rawInput) {
    process.exit(0)
  }

  let input
  try {
    input = JSON.parse(rawInput)
  } catch {
    process.stderr.write(`agentforge-relay-hook: invalid JSON input\n`)
    process.exit(1)
  }

  const event = transformEvent(input)
  const eventJson = JSON.stringify(event)

  try {
    await sendViaUds(eventJson)
  } catch (err) {
    process.stderr.write(`agentforge-relay-hook: failed to deliver event: ${err.message}\n`)
  }
}

main().catch((err) => {
  process.stderr.write(`agentforge-relay-hook: unexpected error: ${err?.message ?? err}\n`)
  process.exit(0) // hooks must not block the CLI
})
