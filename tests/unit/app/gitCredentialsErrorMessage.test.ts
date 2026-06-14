import { describe, expect, test } from 'vitest'
import { gitCredentialsErrorMessage } from '@app/features/settings/gitCredentialsErrorMessage'

describe('gitCredentialsErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
    expect(actual).not.toContain('HTTP')
  }

  test('turns invalid token details into code access key guidance', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage('Settings could not save Git credential. Details: invalid token'),
      'Code access could not be saved. Paste a new code access key from GitHub or GitLab, then save again.'
    )
  })

  test('turns validation failures into clear fields to check', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage('Code: 422 Details: invalid provider'),
      'Code access could not be saved. Choose GitHub or GitLab, then save code access again.'
    )
  })

  test('turns invalid address failures into an address step', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage('HTTP 422: invalid host'),
      'Code access could not be saved. Check the GitHub or GitLab address. Leave it blank for github.com or gitlab.com, then save again.'
    )
  })

  test('turns permission failures into an owner or admin next step', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage('HTTP 403'),
      'Refresh Settings to load code access. Ask an owner or admin to let you manage code access.'
    )
  })

  test('turns delete failures into a remove-specific next step', () => {
    const message = gitCredentialsErrorMessage('Settings could not delete Git credential. HTTP 500')

    expectBeginnerMessage(
      message,
      'Code access could not be removed. Refresh Settings, then try again. If it still fails, ask an owner or admin to check code access settings.'
    )
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns load server failures into Settings recovery guidance', () => {
    const message = gitCredentialsErrorMessage('HTTP 500')

    expectBeginnerMessage(
      message,
      'Refresh Settings to load code access. If it still fails, ask an owner or admin to check code access settings.'
    )
  })

  test('turns network failures into a connection step', () => {
    const message = gitCredentialsErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check your connection, then refresh Settings to load code access. Forge could not connect while opening code access.'
    )
    expect(message).not.toContain('service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('starts save network failures with the recovery step', () => {
    const message = gitCredentialsErrorMessage('saving code access failed: Network error')

    expectBeginnerMessage(
      message,
      'Check your connection, then save code access again. Forge could not connect while opening code access.'
    )
    expect(message).not.toContain('Network error')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      gitCredentialsErrorMessage({ statusCode: '429' }),
      'Refresh Settings to load code access. Forge is receiving too many code access requests right now. Wait a minute, then try again.'
    )
  })

  test('turns unknown details into an owner or admin setup step', () => {
    const message = gitCredentialsErrorMessage({ message: 'unexpected vault parse failure' })

    expectBeginnerMessage(
      message,
      'Refresh Settings to load code access. Try again. If it still fails, ask an owner or admin to check code access settings.'
    )
    expect(message).not.toContain('vault')
  })
})
