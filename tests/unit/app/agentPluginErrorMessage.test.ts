import { describe, expect, test } from 'vitest'
import { agentPluginErrorMessage } from '@app/features/agents/model/pluginErrorMessage'

describe('agentPluginErrorMessage', () => {
  test('turns permission errors into operator recovery guidance', () => {
    expect(agentPluginErrorMessage('load', new Error('HTTP 403'))).toBe(
      "Go back to Agents, choose this agent again, then open Tools. Ask an owner or admin to give you access to this agent's tools."
    )
  })

  test('turns structured permission errors into operator recovery guidance', () => {
    const message = agentPluginErrorMessage('load', {
      detail: 'Forbidden: missing plugin permission',
      statusCode: 403,
    })

    expect(message).toBe(
      "Go back to Agents, choose this agent again, then open Tools. Ask an owner or admin to give you access to this agent's tools."
    )
    expect(message).not.toContain('missing plugin permission')
  })

  test('turns role-required save errors into operator recovery guidance', () => {
    const message = agentPluginErrorMessage('save', 'owner role required')

    expect(message).toBe(
      "Ask an owner or admin to give you access to this agent's tools. The switch was returned to its previous setting."
    )
    expect(message).not.toContain('owner role required')
  })

  test('turns structured save conflicts into a wait and retry step', () => {
    const message = agentPluginErrorMessage('save', {
      code: '409',
      reason: 'plugin update already in progress',
    })

    expect(message).toBe(
      'Wait a moment, then try the tool change again. The switch was returned to its previous setting. Another change is still being saved.'
    )
    expect(message).not.toContain('plugin update already in progress')
  })

  test('explains failed saves without exposing transport details', () => {
    const message = agentPluginErrorMessage('save', new Error('HTTP 500'))

    expect(message).toBe(
      "Wait a few minutes, then try the tool change again. The switch was returned to its previous setting. Forge could not finish this tool request right now. If it still fails, ask an owner or admin to check this agent's tool list."
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('platform')
  })

  test('explains structured service failures without raw response details', () => {
    const message = agentPluginErrorMessage('save', {
      error: 'plugin platform gateway stack trace',
      status: 503,
    })

    expect(message).toBe(
      "Wait a few minutes, then try the tool change again. The switch was returned to its previous setting. Forge could not finish this tool request right now. If it still fails, ask an owner or admin to check this agent's tool list."
    )
    expect(message).not.toContain('gateway stack trace')
    expect(message).not.toContain('platform')
  })

  test('maps network failures to retryable next steps', () => {
    const message = agentPluginErrorMessage('load', new TypeError('Failed to fetch'))

    expect(message).toBe(
      "Check your connection, then go back to Agents, choose this agent again, then open Tools again. Forge could not connect while checking this agent's tools."
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('explains unusable tool lists without raw response wording', () => {
    const message = agentPluginErrorMessage('load', new Error('ok: false'))

    expect(message).toBe(
      'Go back to Agents, choose this agent again, then open Tools. If it still fails, ask an owner or admin to check team space tools.'
    )
    expect(message).not.toContain('ok: false')
    expect(message).not.toContain('workspace tools')
    expect(message).not.toContain('platform')
  })

  test('uses agent list recovery guidance for unknown tool load failures', () => {
    const message = agentPluginErrorMessage('load', new Error('plugin registry mismatch'))

    expect(message).toBe(
      "Go back to Agents, choose this agent again, then open Tools. If it still fails, ask an owner or admin to check this agent's tool list."
    )
    expect(message).not.toContain('plugin registry mismatch')
    expect(message).not.toContain('Try again.')
  })

  test('explains unusable save responses with team space tool guidance', () => {
    const message = agentPluginErrorMessage('save', new Error('ok: false'))

    expect(message).toBe(
      "Go back to Agents, choose this agent again, then open Tools and try the tool change again. The switch was returned to its previous setting. Forge could not read this agent's tool list. If it still fails, ask an owner or admin to check team space tools."
    )
    expect(message).not.toContain('ok: false')
    expect(message).not.toContain('workspace tools')
  })
})
