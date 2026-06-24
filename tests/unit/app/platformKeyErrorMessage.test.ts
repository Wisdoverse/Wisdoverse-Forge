import { describe, expect, test } from 'vitest'
import { platformKeyErrorMessage } from '@app/features/settings/platformKeyErrorMessage'

describe('platformKeyErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
    expect(actual).not.toContain('HTTP')
  }

  test('turns permission errors into an owner or admin step', () => {
    expectBeginnerMessage(
      platformKeyErrorMessage(
        'You do not have permission to create the platform API key. Code: 403. Details: Forbidden'
      ),
      'Ask an owner or admin to let you create or remove tool access keys.'
    )
  })

  test('turns role-required errors into an owner or admin step', () => {
    const message = platformKeyErrorMessage('owner role required')

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to let you create or remove tool access keys.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('explains missing names as the next field to fix', () => {
    expectBeginnerMessage(
      platformKeyErrorMessage(
        'Check the required fields for platform API key, then try again. Code: 422. Details: name is required'
      ),
      'Enter the tool or job name, then create this tool access key again.'
    )
  })

  test('maps nested name validation details', () => {
    const message = platformKeyErrorMessage({
      error: { message: 'name is required' },
    })

    expectBeginnerMessage(
      message,
      'Enter the tool or job name, then create this tool access key again.'
    )
    expect(message).not.toContain('name is required')
  })

  test('explains duplicate keys with a safe next action', () => {
    const message = platformKeyErrorMessage('API 409 duplicate key')

    expectBeginnerMessage(
      message,
      'Open Settings and Tool access keys again, check the current key, then choose a different name or remove the old key first.'
    )
    expect(message).not.toContain('Refresh the list')
    expect(message).not.toContain('Tool access key could not be created')
  })

  test('explains network failures in user-facing terms', () => {
    const message = platformKeyErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check your connection, then open Settings and Tool access keys again. Forge could not connect while opening tool access key settings.'
    )
    expect(message).not.toContain('the service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('starts create network failures with the recovery step', () => {
    const message = platformKeyErrorMessage('creating platform key failed: Network error')

    expectBeginnerMessage(
      message,
      'Check your connection, then create this tool access key again. The creation did not finish.'
    )
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('opening tool access key settings')
  })

  test('starts remove network failures with the recovery step', () => {
    const message = platformKeyErrorMessage('removing platform key failed: Network error')

    expectBeginnerMessage(
      message,
      'Check your connection, then remove this tool access key again. The removal did not finish.'
    )
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('opening tool access key settings')
  })

  test('turns temporary failures into a tool access key settings recovery step', () => {
    const message = platformKeyErrorMessage('HTTP 500')

    expectBeginnerMessage(
      message,
      'Open Settings and Tool access keys again. If it still fails, ask an owner or admin to check tool access key settings.'
    )
    expect(message).not.toContain('access key service')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('keeps unformatted service failures on the tool access key recovery path', () => {
    const message = platformKeyErrorMessage(
      new Error('database unavailable while creating invalid name index')
    )

    expectBeginnerMessage(
      message,
      'Open Settings and Tool access keys again, then create this tool access key again. If it still fails, ask an owner or admin to check tool access key settings.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('Enter the tool or job name')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      platformKeyErrorMessage({ statusCode: '429' }),
      'Wait a minute, then open Settings and Tool access keys again. Forge is receiving too many tool access key requests right now.'
    )
  })

  test('turns unknown details into an owner or admin setup step', () => {
    const message = platformKeyErrorMessage({ message: 'unexpected platform key parser detail' })

    expectBeginnerMessage(
      message,
      'Open Settings and Tool access keys again. If it still fails, ask an owner or admin to check tool access key settings.'
    )
    expect(message).not.toContain('parser')
  })

  test('uses a direct create step for unknown create failures', () => {
    const message = platformKeyErrorMessage({ message: 'creating platform key hit parser edge' })

    expectBeginnerMessage(
      message,
      'Create this tool access key again. If it still fails, ask an owner or admin to check tool access key settings.'
    )
    expect(message).not.toContain('Try to')
    expect(message).not.toContain('parser')
  })
})
