import { describe, expect, test } from 'vitest'
import { feedbackErrorMessage } from '@app/entities/context/model/feedbackErrorMessage'

describe('feedbackErrorMessage', () => {
  test('turns network failures into connection guidance', () => {
    const message = feedbackErrorMessage(new Error('Failed to fetch'))

    expect(message).toBe(
      'Check your connection, then save this feedback again. Forge could not connect while saving it.'
    )
    expect(message).not.toContain('Failed to fetch')
  })

  test('maps permission failures without raw API text', () => {
    const message = feedbackErrorMessage(new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission')
    expect(message).toContain('this saved item')
    expect(message).toContain('Ask an owner or admin')
    expect(message).not.toContain('Code:')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
  })

  test('keeps useful validation detail after the operator action', () => {
    const message = feedbackErrorMessage(new Error('HTTP 422: {"message":"Unknown label."}'))

    expect(message).toContain('Choose one feedback option')
    expect(message).not.toContain('Code:')
    expect(message).not.toContain('Details:')
    expect(message).not.toContain('HTTP 422')
  })
})
