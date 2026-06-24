export const PASSWORD_MIN_LENGTH = 12

export type PasswordRuleId = 'length' | 'upper' | 'lower' | 'number' | 'special'

export interface PasswordRuleState {
  id: PasswordRuleId
  label: string
  met: boolean
  missingMessage: string
}

const SYMBOL_PATTERN = /[!@#$%^&*()_+\-=[\]{};':"\\|,.<>/?`~]/

export function passwordRuleStates(password: string): PasswordRuleState[] {
  return [
    {
      id: 'length',
      label: `Use at least ${PASSWORD_MIN_LENGTH} characters for the new password.`,
      met: password.length >= PASSWORD_MIN_LENGTH,
      missingMessage: `Use at least ${PASSWORD_MIN_LENGTH} characters for the new password. Add a few more characters, then try again.`,
    },
    {
      id: 'upper',
      label: 'Add at least one uppercase letter to the password.',
      met: /[A-Z]/.test(password),
      missingMessage: 'Add at least one uppercase letter to the password, then try again.',
    },
    {
      id: 'lower',
      label: 'Add at least one lowercase letter to the password.',
      met: /[a-z]/.test(password),
      missingMessage: 'Add at least one lowercase letter to the password, then try again.',
    },
    {
      id: 'number',
      label: 'Add at least one number to the password.',
      met: /[0-9]/.test(password),
      missingMessage: 'Add at least one number to the password, then try again.',
    },
    {
      id: 'special',
      label: 'Add at least one symbol to the password.',
      met: SYMBOL_PATTERN.test(password),
      missingMessage: 'Add at least one symbol to the password, then try again.',
    },
  ]
}

export function passwordRuleMessage(password: string, retryAction = 'try again'): string | null {
  return (
    passwordRuleStates(password)
      .find((rule) => !rule.met)
      ?.missingMessage.replace('try again', retryAction) ?? null
  )
}
