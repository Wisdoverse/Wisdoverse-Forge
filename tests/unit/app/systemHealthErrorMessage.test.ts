import { describe, expect, test } from 'vitest'
import { systemHealthErrorMessage } from '@app/features/admin/systemHealthErrorMessage'

describe('systemHealthErrorMessage', () => {
  test('turns auth failures into a sign-in next step', () => {
    expect(systemHealthErrorMessage('Code: 401.')).toBe(
      'Service readiness could not be loaded. Sign in again, then open Admin and check service readiness.'
    )
  })

  test('turns server failures into a backend health next step', () => {
    expect(systemHealthErrorMessage('HTTP 500')).toBe(
      'Service readiness could not be loaded. The admin API is temporarily unavailable. Check the backend service, then choose Check now.'
    )
  })

  test('turns network failures into a connection next step', () => {
    expect(systemHealthErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Service readiness could not be loaded. The browser could not reach the server. Check your connection or API route, then choose Check now.'
    )
  })
})
