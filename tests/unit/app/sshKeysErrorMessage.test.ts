import { describe, expect, test } from 'vitest'
import { sshKeysErrorMessage } from '@app/features/settings/sshKeysErrorMessage'

describe('sshKeysErrorMessage', () => {
  test('turns invalid public key errors into a clear recovery step', () => {
    expect(
      sshKeysErrorMessage('Settings could not save SSH key. Details: invalid public key')
    ).toBe(
      'Repository SSH access could not be saved. Paste only the shareable public line that starts with ssh-ed25519 or ssh-rsa, then save again.'
    )
  })

  test('explains permission errors without exposing raw backend details', () => {
    expect(sshKeysErrorMessage('HTTP 403')).toBe(
      'Repository SSH access could not be loaded. Ask an owner or admin for access to manage repository SSH access.'
    )
  })

  test('explains duplicate keys with a safe next action', () => {
    expect(sshKeysErrorMessage('API 409 duplicate key')).toBe(
      'Repository SSH access could not be loaded. This public line already exists. Choose the saved access or remove the old one first.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    const message = sshKeysErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Repository SSH access could not be loaded. The app could not reach repository SSH access. Check your connection, then try again.'
    )
    expect(message).not.toContain('service')
  })
})
