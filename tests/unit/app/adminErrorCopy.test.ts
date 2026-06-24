import { describe, expect, test } from 'vitest'
import {
  adminPanelLoadErrorMessage,
  cliImageStatusErrorMessage,
} from '@app/features/admin/adminErrorCopy'

describe('adminErrorCopy', () => {
  test('hides backend details without status codes in admin panel load errors', () => {
    const message = adminPanelLoadErrorMessage('database unavailable', 'user list')

    expect(message).toBe('Open Admin again, then choose user list.')
    expect(message).not.toContain('database unavailable')
  })

  test('hides raw server error names in tool update status errors', () => {
    const message = cliImageStatusErrorMessage('Internal Server Error')

    expect(message).toBe('Choose Check now to load tool update status.')
    expect(message).not.toContain('Internal Server Error')
  })
})
