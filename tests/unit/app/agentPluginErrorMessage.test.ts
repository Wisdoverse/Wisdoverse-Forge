import { describe, expect, test } from 'vitest'
import { agentPluginErrorMessage } from '@app/features/agents/model/pluginErrorMessage'

describe('agentPluginErrorMessage', () => {
  test('turns permission errors into operator recovery guidance', () => {
    expect(agentPluginErrorMessage('load', new Error('HTTP 403'))).toBe(
      "Agent tools could not be loaded. Ask an owner or admin to give you access to this agent's tools."
    )
  })

  test('explains failed saves without exposing transport details', () => {
    const message = agentPluginErrorMessage('save', new Error('HTTP 500'))

    expect(message).toBe(
      "Tool change was not saved. The switch was returned to its previous setting. Forge could not finish this tool request right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check this agent's tool setup."
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('platform')
  })

  test('maps network failures to retryable next steps', () => {
    const message = agentPluginErrorMessage('load', new TypeError('Failed to fetch'))

    expect(message).toBe(
      "Agent tools could not be loaded. Forge could not connect while checking this agent's tools. Check your connection, then try again."
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('explains unusable tool lists without raw response wording', () => {
    const message = agentPluginErrorMessage('load', new Error('ok: false'))

    expect(message).toBe(
      "Agent tools could not be loaded. Forge could not read this agent's tool list. Refresh the page. If it still fails, ask an owner or admin to check workspace tools."
    )
    expect(message).not.toContain('ok: false')
    expect(message).not.toContain('platform')
  })
})
