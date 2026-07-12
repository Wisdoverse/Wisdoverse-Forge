import { describe, expect, it } from 'vitest'
import { waitingPlaceDisplayName } from '@app/entities/navigation/agent-group'

describe('waitingPlaceDisplayName', () => {
  it('turns queue-era names into beginner-facing task-queue names', () => {
    expect(waitingPlaceDisplayName('Delivery Queue')).toBe('Delivery task queue')
    expect(waitingPlaceDisplayName('Review task queue')).toBe('Review task queue')
    expect(waitingPlaceDisplayName('QA Queues')).toBe('QA task queue')
  })

  it('uses a plain fallback for blank names', () => {
    expect(waitingPlaceDisplayName('  ')).toBe('this task queue')
    expect(waitingPlaceDisplayName(null)).toBe('this task queue')
  })
})
