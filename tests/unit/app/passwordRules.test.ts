import { describe, expect, test } from 'vitest'

import { passwordRuleMessage } from '@app/shared/lib/passwordRules'

describe('passwordRuleMessage', () => {
  test('uses a concrete default save action for missing password rules', () => {
    const message = passwordRuleMessage('short')

    expect(message).toBe(
      'Use at least 12 characters for the new password. Add a few more characters, then save the password again.'
    )
    expect(message).not.toContain('try again')
  })

  test('keeps caller-specific password retry actions', () => {
    expect(passwordRuleMessage('short', 'choose Save new password again')).toBe(
      'Use at least 12 characters for the new password. Add a few more characters, then choose Save new password again.'
    )
  })
})
