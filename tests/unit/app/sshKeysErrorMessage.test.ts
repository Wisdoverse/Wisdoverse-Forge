import { describe, expect, test } from 'vitest'
import { sshKeysErrorMessage } from '@app/features/settings/sshKeysErrorMessage'

describe('sshKeysErrorMessage', () => {
  test('turns invalid public key errors into a clear recovery step', () => {
    expect(sshKeysErrorMessage('Settings could not save SSH key. Details: invalid public key')).toBe(
      'Repository SSH key could not be saved. Paste only the public key line that starts with ssh-ed25519 or ssh-rsa, then save again.'
    )
  })

  test('explains permission errors without exposing raw backend details', () => {
    expect(sshKeysErrorMessage('HTTP 403')).toBe(
      'Repository SSH keys could not be loaded. Ask an owner or admin for access to manage repository SSH keys.'
    )
  })

  test('explains duplicate keys with a safe next action', () => {
    expect(sshKeysErrorMessage('API 409 duplicate key')).toBe(
      'Repository SSH keys could not be loaded. This public key already exists. Choose the saved key or remove the old one first.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    expect(sshKeysErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Repository SSH keys could not be loaded. The browser could not reach the server. Check your connection, then try again.'
    )
  })
})
