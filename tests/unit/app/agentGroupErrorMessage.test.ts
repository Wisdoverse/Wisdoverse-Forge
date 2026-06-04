import { describe, expect, test } from 'vitest'
import { agentGroupErrorMessage } from '@app/features/agents/model/agentGroupErrorMessage'

describe('agentGroupErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(agentGroupErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      "Work lane was not created. Ask a workspace owner or admin to let you manage this project's work lanes."
    )
  })

  test('explains naming conflicts without leaking raw API wording', () => {
    expect(agentGroupErrorMessage(new Error('API 409 lane conflict'))).toBe(
      'Work lane was not created. A lane with this name may already exist. Use a different name, then try again.'
    )
  })

  test('gives a connection recovery path for network failures', () => {
    expect(agentGroupErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Work lane was not created. Check your connection, then try again.'
    )
  })
})
