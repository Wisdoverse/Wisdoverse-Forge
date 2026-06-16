import { describe, expect, test } from 'vitest'
import { systemHealthErrorMessage } from '@app/features/admin/systemHealthErrorMessage'

describe('systemHealthErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('HTTP')
    expect(actual).not.toContain('API')
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('service readiness')
    expect(actual).not.toContain(['app', 'readiness'].join(' '))
  }

  test('turns auth failures into a sign-in next step', () => {
    expectBeginnerMessage(
      systemHealthErrorMessage('Code: 401.'),
      'Sign in again, then open Admin and choose Check now. Forge could not check app health because your sign-in expired.'
    )
  })

  test('turns permission failures into an Admin access next step', () => {
    expectBeginnerMessage(
      systemHealthErrorMessage({ status: 403, detail: 'Forbidden' }),
      'Ask an owner or admin to give you Admin access, then choose Check now. Forge could not check app health because you do not have access to app health checks.'
    )
  })

  test('turns missing routes into an Admin view recovery step', () => {
    const message = systemHealthErrorMessage({ statusCode: '404' })

    expectBeginnerMessage(
      message,
      'Refresh Admin, then choose Check now. App health checks are not available from this Admin view. If it still fails, ask an owner or admin to check setup.'
    )
    expect(message).not.toContain('endpoint')
    expect(message).not.toContain('route')
  })

  test('turns rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      systemHealthErrorMessage({ code: '429' }),
      'Wait a minute, then choose Check now. Forge is receiving too many health checks right now.'
    )
  })

  test('turns server failures into a setup recovery step', () => {
    const message = systemHealthErrorMessage('HTTP 500')

    expectBeginnerMessage(
      message,
      'Refresh Admin, then choose Check now. Forge could not check app health. If it still fails, ask an owner or admin to check app health setup.'
    )
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('admin service')
  })

  test('turns network failures into a connection next step', () => {
    const message = systemHealthErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check your connection, then choose Check now. Forge could not connect while checking app health.'
    )
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('browser could not reach')
    expect(message).not.toContain('service.')
  })

  test('turns unknown failures into a retry and setup next step', () => {
    const message = systemHealthErrorMessage({ message: 'unexpected parser error' })

    expectBeginnerMessage(
      message,
      'Choose Check now again. Forge could not check app health. If it still fails, ask an owner or admin to check app health setup.'
    )
    expect(message).not.toContain('parser')
  })
})
