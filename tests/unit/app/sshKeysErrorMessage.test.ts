import { describe, expect, test } from 'vitest'
import { sshKeysErrorMessage } from '@app/features/settings/sshKeysErrorMessage'

describe('sshKeysErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
    expect(actual).not.toContain('HTTP')
  }

  test('turns invalid public key errors into a clear recovery step', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage('Settings could not save SSH key. Details: invalid public key'),
      'git@ repository access could not be saved. Paste only the shareable one-line SSH key that starts with ssh-ed25519 or ssh-rsa, then save again. Do not paste a private key block.'
    )
  })

  test('explains permission errors without exposing raw backend details', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage('HTTP 403'),
      'git@ repository access could not be loaded. Ask an owner or admin for access to manage git@ repository access.'
    )
  })

  test('explains duplicate keys with a safe next action', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage('API 409 duplicate key'),
      'git@ repository access could not be saved. This public SSH key line is already saved. Choose the saved access or remove the old one first.'
    )
  })

  test('explains missing fields as the next form fields to fix', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage('Code: 422 Details: public key is required'),
      'git@ repository access could not be saved. Paste the public SSH key line that starts with ssh-ed25519 or ssh-rsa, then save again.'
    )
  })

  test('keeps Settings store validation messages on the save path', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage(
        'Add a label, paste a valid public SSH key, then save the SSH key again.'
      ),
      'git@ repository access could not be saved. Add a name for this access, then save again.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    const message = sshKeysErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'git@ repository access could not be loaded. Forge could not connect while opening git@ repository access. Check your connection, then try again.'
    )
    expect(message).not.toContain('service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns server failures into Settings recovery guidance', () => {
    const message = sshKeysErrorMessage({ status: 503 })

    expectBeginnerMessage(
      message,
      'git@ repository access could not be loaded. Refresh Settings, then try again. If it still fails, ask an owner or admin to check git@ repository access settings.'
    )
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage({ code: '429' }),
      'git@ repository access could not be loaded. Forge is receiving too many git@ repository access requests right now. Wait a minute, then try again.'
    )
  })

  test('turns unknown details into an owner or admin setup step', () => {
    const message = sshKeysErrorMessage({ reason: 'unexpected key parser detail' })

    expectBeginnerMessage(
      message,
      'git@ repository access could not be loaded. Try again. If it still fails, ask an owner or admin to check git@ repository access settings.'
    )
    expect(message).not.toContain('parser')
  })
})
