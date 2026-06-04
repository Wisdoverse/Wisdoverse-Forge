import { describe, expect, test } from 'vitest'
import { workspaceResourceErrorMessage } from '@app/shared/lib/workspaceResourceErrorMessage'

describe('workspaceResourceErrorMessage', () => {
  test('turns network failures into connection guidance', () => {
    const message = workspaceResourceErrorMessage('team', 'update', new Error('Failed to fetch'))

    expect(message).toContain('browser could not reach the server')
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
  })

  test('maps project permission failures without raw API text', () => {
    const message = workspaceResourceErrorMessage('project', 'delete', new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission')
    expect(message).toContain('Ask an owner or admin')
    expect(message).toContain('Code: 403.')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
  })

  test('keeps useful validation detail for team delete blockers', () => {
    const message = workspaceResourceErrorMessage(
      'team',
      'delete',
      new Error('HTTP 422: {"message":"Move projects first."}')
    )

    expect(message).toContain('This team could not be deleted')
    expect(message).toContain('Code: 422.')
    expect(message).toContain('Details: Move projects first.')
    expect(message).not.toContain('HTTP 422')
  })
})
