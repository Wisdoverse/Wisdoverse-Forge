import { describe, expect, it } from 'vitest'

import { taskPriorityLabel, taskStateLabel } from '@app/entities/task'

describe('task labels', () => {
  it('uses plain failed-task wording', () => {
    const label = taskStateLabel('failed')

    expect(label).toBe('Needs another try')
    expect(label).not.toBe('Needs a retry')
  })

  it('keeps unknown values safe for users', () => {
    expect(taskStateLabel('waiting_for_agent')).toBe('Open task details to read this status')
    expect(taskPriorityLabel('future_priority')).toBe('Open task details to read this priority')
  })
})
