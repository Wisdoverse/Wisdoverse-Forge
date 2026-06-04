import { describe, expect, test } from 'vitest'
import { agentPluginErrorMessage } from '@app/features/agents/model/pluginErrorMessage'

describe('agentPluginErrorMessage', () => {
  test('turns permission errors into operator recovery guidance', () => {
    expect(agentPluginErrorMessage('load', new Error('HTTP 403'))).toBe(
      'Agent tools could not be loaded. Ask a workspace owner or admin to give you permission for this agent.'
    )
  })

  test('explains failed saves without exposing transport details', () => {
    const message = agentPluginErrorMessage('save', new Error('HTTP 500'))

    expect(message).toBe(
      'Tool change was not saved. The switch was returned to its previous setting. The platform is temporarily unavailable. Try again in a few minutes.'
    )
    expect(message).not.toContain('HTTP 500')
  })

  test('maps network failures to retryable next steps', () => {
    expect(agentPluginErrorMessage('load', new TypeError('Failed to fetch'))).toBe(
      'Agent tools could not be loaded. Check your connection, then try again.'
    )
  })
})
