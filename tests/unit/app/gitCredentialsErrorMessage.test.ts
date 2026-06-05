import { describe, expect, test } from 'vitest'
import { gitCredentialsErrorMessage } from '@app/features/settings/gitCredentialsErrorMessage'

describe('gitCredentialsErrorMessage', () => {
  test('turns invalid token details into a repository access key replacement step', () => {
    expect(
      gitCredentialsErrorMessage('Settings could not save Git credential. Details: invalid token')
    ).toBe(
      'Repository access could not be saved. Paste a new repository access key from GitHub or GitLab, then save again.'
    )
  })

  test('turns permission failures into an owner or admin next step', () => {
    expect(gitCredentialsErrorMessage('HTTP 403')).toBe(
      'Repository access could not be loaded. Ask an owner or admin to let you manage repository access.'
    )
  })

  test('turns delete failures into a remove-specific next step', () => {
    expect(gitCredentialsErrorMessage('Settings could not delete Git credential. HTTP 500')).toBe(
      'Repository access could not be removed. The repository access service is temporarily unavailable. Try again. If it still fails, ask an owner to check repository access settings.'
    )
  })

  test('turns network failures into a connection step', () => {
    expect(gitCredentialsErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Repository access could not be loaded. The app could not reach the service. Check your connection, then try again.'
    )
  })
})
