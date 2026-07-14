import { describe, expect, it } from 'vitest'
import { waitingPlaceDisplayName } from '@app/entities/navigation/agent-group'

describe('waitingPlaceDisplayName', () => {
  it('turns queue-era names into beginner-facing place names', () => {
    expect(waitingPlaceDisplayName('Delivery Queue')).toBe('Delivery place')
    expect(waitingPlaceDisplayName('Review task queue')).toBe('Review place')
    expect(waitingPlaceDisplayName('QA Queues')).toBe('QA place')
  })

  it('uses a plain fallback for blank names', () => {
    expect(waitingPlaceDisplayName('  ')).toBe('this place')
    expect(waitingPlaceDisplayName(null)).toBe('this place')
  })
})
