import { describe, expect, test } from 'vitest'
import { systemHealthErrorMessage } from '@app/features/admin/systemHealthErrorMessage'

describe('systemHealthErrorMessage', () => {
  test('turns auth failures into a sign-in next step', () => {
    expect(systemHealthErrorMessage('Code: 401.')).toBe(
      'Service readiness could not be loaded. Sign in again, then open Admin and check service readiness.'
    )
  })

  test('turns server failures into an admin service next step', () => {
    expect(systemHealthErrorMessage('HTTP 500')).toBe(
      'Service readiness could not be loaded. Service readiness is temporarily unavailable. Ask an owner to check the admin service, then choose Check now.'
    )
  })

  test('turns network failures into a connection next step', () => {
    expect(systemHealthErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Service readiness could not be loaded. The browser could not reach the service. Check your connection, then choose Check now.'
    )
  })
})
