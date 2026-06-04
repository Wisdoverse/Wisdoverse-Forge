import { describe, expect, test } from 'vitest'
import { gitCredentialsErrorMessage } from '@app/features/settings/gitCredentialsErrorMessage'

describe('gitCredentialsErrorMessage', () => {
  test('turns invalid token details into a token replacement step', () => {
    expect(
      gitCredentialsErrorMessage('Settings could not save Git credential. Details: invalid token')
    ).toBe(
      'Repository token could not be saved. Paste a new token from GitHub or GitLab with repository access, then save again.'
    )
  })

  test('turns permission failures into an owner or admin next step', () => {
    expect(gitCredentialsErrorMessage('HTTP 403')).toBe(
      'Repository access tokens could not be loaded. Ask an owner or admin for access to manage repository tokens.'
    )
  })

  test('turns delete failures into a remove-specific next step', () => {
    expect(gitCredentialsErrorMessage('Settings could not delete Git credential. HTTP 500')).toBe(
      'Repository token could not be removed. The repository access service is temporarily unavailable. Try again. If it still fails, ask an owner to check repository access settings.'
    )
  })

  test('turns network failures into a connection step', () => {
    expect(gitCredentialsErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Repository access tokens could not be loaded. The browser could not reach the server. Check your connection, then try again.'
    )
  })
})
