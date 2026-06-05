import { describe, expect, test } from 'vitest'
import { systemHealthErrorMessage } from '@app/features/admin/systemHealthErrorMessage'

describe('systemHealthErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('HTTP')
    expect(actual).not.toContain('API')
    expect(actual).not.toContain('Code:')
  }

  test('turns auth failures into a sign-in next step', () => {
    expectBeginnerMessage(
      systemHealthErrorMessage('Code: 401.'),
      'Forge could not check service readiness. Your sign-in expired. Sign in again, then open Admin and choose Check now.'
    )
  })

  test('turns permission failures into a role update next step', () => {
    expectBeginnerMessage(
      systemHealthErrorMessage({ status: 403, detail: 'Forbidden' }),
      'Forge could not check service readiness. You do not have access to service readiness. Ask an owner or admin to update your role, then choose Check now.'
    )
  })

  test('turns missing routes into an Admin view recovery step', () => {
    const message = systemHealthErrorMessage({ statusCode: '404' })

    expectBeginnerMessage(
      message,
      'Forge could not check service readiness. Service readiness is not available from this Admin view. Refresh Admin, then choose Check now. If it still fails, ask an owner or admin to check setup.'
    )
    expect(message).not.toContain('endpoint')
    expect(message).not.toContain('route')
  })

  test('turns rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      systemHealthErrorMessage({ code: '429' }),
      'Forge could not check service readiness. Forge is receiving too many readiness checks right now. Wait a minute, then choose Check now.'
    )
  })

  test('turns server failures into a setup recovery step', () => {
    const message = systemHealthErrorMessage('HTTP 500')

    expectBeginnerMessage(
      message,
      'Forge could not check service readiness. Refresh Admin, then choose Check now. If it still fails, ask an owner or admin to check service readiness setup.'
    )
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('admin service')
  })

  test('turns network failures into a connection next step', () => {
    const message = systemHealthErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Forge could not check service readiness. Forge could not connect while checking service readiness. Check your connection, then choose Check now.'
    )
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('browser could not reach')
    expect(message).not.toContain('service.')
  })

  test('turns unknown failures into a retry and setup next step', () => {
    const message = systemHealthErrorMessage({ message: 'unexpected parser error' })

    expectBeginnerMessage(
      message,
      'Forge could not check service readiness. Choose Check now again. If it still fails, ask an owner or admin to check service readiness setup.'
    )
    expect(message).not.toContain('parser')
  })
})
