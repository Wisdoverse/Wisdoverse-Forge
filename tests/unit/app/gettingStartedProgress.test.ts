import { describe, expect, test } from 'vitest'
import { getGettingStartedProgress } from '@app/shared/lib/gettingStartedProgress'

describe('getGettingStartedProgress', () => {
  test('counts the shared eight-step checklist criteria', () => {
    const progress = getGettingStartedProgress({
      hasWorkspace: true,
      runtimeReady: true,
      executionCredentialReady: false,
      hasAgent: true,
      hasRouting: true,
      taskSnapshot: { total: 1, assigned: 1, completed: 1, artifacts: 0, appliedSkills: 0 },
      hasReusableLearning: false,
    })

    expect(progress.completeCount).toBe(6)
    expect(progress.total).toBe(8)
    expect(progress.completion.provider).toBe(false)
    expect(progress.completion.review).toBe(true)
  })
})
