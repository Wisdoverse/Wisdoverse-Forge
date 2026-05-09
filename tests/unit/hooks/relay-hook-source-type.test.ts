import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import fs from 'node:fs'
import path from 'node:path'
import Module from 'node:module'

function loadTransformEvent() {
  const hookPath = path.resolve(__dirname, '../../../hooks/agentforge-relay-hook.cjs')
  const source = fs.readFileSync(hookPath, 'utf8')

  // Extract everything before main() — contains transformEvent and helpers
  const mainIndex = source.indexOf('async function main()')
  const moduleCode = source
    .slice(0, mainIndex)
    // Strip shebang line
    .replace(/^#!.*\n/, '')

  // The source already has 'use strict' and require() calls — just append exports
  const wrappedCode = `${moduleCode}
    module.exports = { transformEvent, detectSourceHookType };
  `

  const m = new Module('relay-hook-test')
  m._compile(wrappedCode, hookPath)
  return m.exports as {
    transformEvent: (input: Record<string, unknown>) => Record<string, unknown>
    detectSourceHookType: (hookEventName: string) => string
  }
}

describe('agentforge-relay-hook sourceHookType', () => {
  const originalEnv = process.env.AGENTFORGE_CLI_TOOL

  beforeEach(() => {
    delete process.env.AGENTFORGE_CLI_TOOL
  })

  afterEach(() => {
    if (originalEnv !== undefined) {
      process.env.AGENTFORGE_CLI_TOOL = originalEnv
    } else {
      delete process.env.AGENTFORGE_CLI_TOOL
    }
  })

  it('should set sourceHookType to "claude" for Claude Code events', () => {
    const { transformEvent } = loadTransformEvent()
    const claudeEvents = [
      'PreToolUse',
      'PostToolUse',
      'Stop',
      'SubagentStop',
      'SessionStart',
      'SessionEnd',
      'UserPromptSubmit',
      'Notification',
      'PreCompact',
    ]

    for (const hookName of claudeEvents) {
      const result = transformEvent({ hook_event_name: hookName, session_id: 'test' })
      expect(result.sourceHookType).toBe('claude')
    }
  })

  it('should set sourceHookType to "gemini" for Gemini CLI events', () => {
    const { transformEvent } = loadTransformEvent()
    const geminiEvents = ['BeforeTool', 'AfterTool', 'AfterAgent', 'BeforeAgent', 'PreCompress']

    for (const hookName of geminiEvents) {
      const result = transformEvent({ hook_event_name: hookName, session_id: 'test' })
      expect(result.sourceHookType).toBe('gemini')
    }
  })

  it('should set sourceHookType to "unknown" for unrecognized events', () => {
    const { transformEvent } = loadTransformEvent()
    const result = transformEvent({ hook_event_name: 'SomeNewEvent', session_id: 'test' })
    expect(result.sourceHookType).toBe('unknown')
  })

  it('should use AGENTFORGE_CLI_TOOL env var as override', () => {
    process.env.AGENTFORGE_CLI_TOOL = 'codex'
    const { transformEvent } = loadTransformEvent()

    // Even though PreToolUse is a Claude event, env var should override
    const result = transformEvent({ hook_event_name: 'PreToolUse', session_id: 'test' })
    expect(result.sourceHookType).toBe('codex')
  })

  it('should use AGENTFORGE_CLI_TOOL env var for opencode', () => {
    process.env.AGENTFORGE_CLI_TOOL = 'opencode'
    const { transformEvent } = loadTransformEvent()

    const result = transformEvent({ hook_event_name: 'BeforeTool', session_id: 'test' })
    expect(result.sourceHookType).toBe('opencode')
  })

  it('should classify Codex-only native permission requests as Codex events', () => {
    const { transformEvent } = loadTransformEvent()

    const result = transformEvent({
      hook_event_name: 'PermissionRequest',
      session_id: 'test',
      tool_name: 'Bash',
      tool_input: { description: 'Run make prod-ext', command: 'make prod-ext' },
    })

    expect(result.type).toBe('permission_request')
    expect(result.sourceHookType).toBe('codex')
    expect(result.cliTool).toBe('codex')
    expect(result.tool).toBe('Bash')
    expect(result.description).toBe('Run make prod-ext')
  })
})
