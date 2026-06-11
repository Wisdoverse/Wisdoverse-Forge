import { describe, expect, test } from 'vitest'
import { gitCredentialsErrorMessage } from '@app/features/settings/gitCredentialsErrorMessage'

describe('gitCredentialsErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
    expect(actual).not.toContain('HTTP')
  }

  test('turns invalid token details into repository access key guidance', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage('Settings could not save Git credential. Details: invalid token'),
      'Repository access could not be saved. Paste a new repository access key from GitHub or GitLab, then save again.'
    )
  })

  test('turns validation failures into clear fields to check', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage('Code: 422 Details: invalid provider'),
      'Repository access could not be saved. Choose GitHub or GitLab, then save repository access again.'
    )
  })

  test('turns invalid address failures into an address step', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage('HTTP 422: invalid host'),
      'Repository access could not be saved. Check the GitHub or GitLab address. Leave it blank for github.com or gitlab.com, then save again.'
    )
  })

  test('turns permission failures into an owner or admin next step', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage('HTTP 403'),
      'Repository access could not be loaded. Ask an owner or admin to let you manage repository access.'
    )
  })

  test('turns delete failures into a remove-specific next step', () => {
    const message = gitCredentialsErrorMessage('Settings could not delete Git credential. HTTP 500')

    expectBeginnerMessage(
      message,
      'Repository access could not be removed. Refresh Settings, then try again. If it still fails, ask an owner or admin to check repository access settings.'
    )
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns network failures into a connection step', () => {
    const message = gitCredentialsErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Repository access could not be loaded. Forge could not connect while opening repository access. Check your connection, then try again.'
    )
    expect(message).not.toContain('service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage({ statusCode: '429' }),
      'Repository access could not be loaded. Forge is receiving too many repository access requests right now. Wait a minute, then try again.'
    )
  })

  test('turns unknown details into an owner or admin setup step', () => {
    const message = gitCredentialsErrorMessage({ message: 'unexpected vault parse failure' })

    expectBeginnerMessage(
      message,
      'Repository access could not be loaded. Try again. If it still fails, ask an owner or admin to check repository access settings.'
    )
    expect(message).not.toContain('vault')
  })
})
