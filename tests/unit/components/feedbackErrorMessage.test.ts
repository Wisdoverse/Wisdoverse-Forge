import { describe, expect, test } from 'vitest'
import { feedbackErrorMessage } from '@app/entities/context/model/feedbackErrorMessage'

describe('feedbackErrorMessage', () => {
  test('turns network failures into connection guidance', () => {
    const message = feedbackErrorMessage(new Error('Failed to fetch'))

    expect(message).toContain('browser could not reach the server')
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
  })

  test('maps permission failures without raw API text', () => {
    const message = feedbackErrorMessage(new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission')
    expect(message).toContain('Ask an admin')
    expect(message).toContain('Code: 403.')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
  })

  test('keeps useful validation detail after the operator action', () => {
    const message = feedbackErrorMessage(new Error('HTTP 422: {"message":"Unknown label."}'))

    expect(message).toContain('Choose one feedback option')
    expect(message).toContain('Code: 422.')
    expect(message).toContain('Details: Unknown label.')
    expect(message).not.toContain('HTTP 422')
  })
})
