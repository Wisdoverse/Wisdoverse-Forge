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
      'Paste only the safe one-line public key from the .pub file, then save again. Do not paste a private key block.'
    )
  })

  test('maps nested invalid public key details', () => {
    const message = sshKeysErrorMessage({
      error: { message: 'invalid public key' },
    })

    expectBeginnerMessage(
      message,
      'Paste only the safe one-line public key from the .pub file, then save again. Do not paste a private key block.'
    )
    expect(message).not.toContain('invalid public key')
  })

  test('explains permission errors without exposing raw backend details', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage('HTTP 403'),
      'Ask an owner or admin for access to manage code access for SSH links.'
    )
  })

  test('explains role-required errors without treating them as missing fields', () => {
    const message = sshKeysErrorMessage('owner role required')

    expectBeginnerMessage(
      message,
      'Ask an owner or admin for access to manage code access for SSH links.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('access name')
  })

  test('explains duplicate keys with a safe next action', () => {
    const message = sshKeysErrorMessage('API 409 duplicate key')

    expectBeginnerMessage(
      message,
      'Choose the saved access or remove the old one first. This safe public key line is already saved.'
    )
    expect(message).not.toContain('could not be saved')
  })

  test('explains missing fields as the next form fields to fix', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage('Code: 422 Details: public key is required'),
      'Paste the safe public key line from the .pub file, then save again.'
    )
  })

  test('turns generic missing SSH-link code access fields into a save step', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage({ status: 422, reason: 'name and key are required' }),
      'Check the access name and safe public key line, then save this code access for SSH links again.'
    )
  })

  test('keeps Settings store validation messages on the save path', () => {
    const message = sshKeysErrorMessage(
      'Add a label, paste a valid public SSH key, then save the SSH key again.'
    )

    expectBeginnerMessage(message, 'Add a name for this access, then save again.')
    expect(message).not.toContain('could not be saved')
  })

  test('explains network failures in user-facing terms', () => {
    const message = sshKeysErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check your connection, then open Settings and code access for SSH links again. Forge could not connect while opening code access for SSH links.'
    )
    expect(message).not.toContain('service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('starts save network failures with the recovery step', () => {
    const message = sshKeysErrorMessage('saving SSH key failed: Network error')

    expectBeginnerMessage(
      message,
      'Check your connection, then save this code access for SSH links again. The save did not finish.'
    )
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('opening SSH code access')
  })

  test('starts remove network failures with the recovery step', () => {
    const message = sshKeysErrorMessage('removing SSH key failed: Network error')

    expectBeginnerMessage(
      message,
      'Check your connection, then remove this code access for SSH links again. The removal did not finish.'
    )
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('opening SSH code access')
  })

  test('turns server failures into Settings recovery guidance', () => {
    const message = sshKeysErrorMessage({ status: 503 })

    expectBeginnerMessage(
      message,
      'Open Settings and code access for SSH links again. If it still fails, ask an owner or admin to check code access for SSH links.'
    )
    expect(message).not.toContain('temporarily unavailable')
  })

  test('keeps unformatted service failures on the SSH access recovery path', () => {
    const message = sshKeysErrorMessage(new Error('database unavailable while saving public key'))

    expectBeginnerMessage(
      message,
      'Open Settings and code access for SSH links again, then save this code access for SSH links again. If it still fails, ask an owner or admin to check code access for SSH links.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('Paste the safe public key')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      sshKeysErrorMessage({ code: '429' }),
      'Wait a minute, then open Settings and code access for SSH links again. Forge is receiving too many requests for code access for SSH links right now.'
    )
  })

  test('turns unknown details into an owner or admin setup step', () => {
    const message = sshKeysErrorMessage({ reason: 'unexpected key parser detail' })

    expectBeginnerMessage(
      message,
      'Open Settings and code access for SSH links again. If it still fails, ask an owner or admin to check code access for SSH links.'
    )
    expect(message).not.toContain('parser')
  })

  test('uses a direct save step for unknown save failures', () => {
    const message = sshKeysErrorMessage({ reason: 'saving access hit parser edge' })

    expectBeginnerMessage(
      message,
      'Save this code access for SSH links again. If it still fails, ask an owner or admin to check code access for SSH links.'
    )
    expect(message).not.toContain('Try to')
    expect(message).not.toContain('parser')
  })
})
