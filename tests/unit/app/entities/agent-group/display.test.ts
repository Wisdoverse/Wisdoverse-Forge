import { describe, expect, it } from 'vitest'
import { waitingPlaceDisplayName } from '@app/entities/agent-group'

describe('waitingPlaceDisplayName', () => {
  it('turns queue-era names into beginner-facing waiting-place names', () => {
    expect(waitingPlaceDisplayName('Delivery Queue')).toBe('Delivery waiting place')
    expect(waitingPlaceDisplayName('Review task queue')).toBe('Review waiting place')
    expect(waitingPlaceDisplayName('QA Queues')).toBe('QA waiting place')
  })

  it('uses a plain fallback for blank names', () => {
    expect(waitingPlaceDisplayName('  ')).toBe('this place')
    expect(waitingPlaceDisplayName(null)).toBe('this place')
  })
})
