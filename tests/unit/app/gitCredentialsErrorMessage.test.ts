import { describe, expect, test } from 'vitest'
import { gitCredentialsErrorMessage } from '@app/features/settings/gitCredentialsErrorMessage'

describe('gitCredentialsErrorMessage', () => {
  test('turns invalid token details into an access key replacement step', () => {
    expect(
      gitCredentialsErrorMessage('Settings could not save Git credential. Details: invalid token')
    ).toBe(
      'Repository access could not be saved. Paste a new GitHub or GitLab access key, then save again.'
    )
  })

  test('turns permission failures into an owner or admin next step', () => {
    expect(gitCredentialsErrorMessage('HTTP 403')).toBe(
      'Repository access could not be loaded. Ask an owner or admin to let you manage repository access.'
    )
  })

  test('turns delete failures into a remove-specific next step', () => {
    expect(gitCredentialsErrorMessage('Settings could not delete Git credential. HTTP 500')).toBe(
      'Repository access could not be removed. Repository access is temporarily unavailable. Try again. If it still fails, ask an owner to check repository access settings.'
    )
  })

  test('turns network failures into a connection step', () => {
    const message = gitCredentialsErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Repository access could not be loaded. The app could not reach repository access. Check your connection, then try again.'
    )
    expect(message).not.toContain('service')
  })
})
