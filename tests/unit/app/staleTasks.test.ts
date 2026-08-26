import { describe, expect, test } from 'vitest'
import { isStaleTask, staleTaskCount, STALE_RETIRE_DAYS } from '@app/features/board/model/staleTasks'
import type { TaskSummary } from '@app/shared/api/orchestration'

const days = 86_400_000

function task(overrides: Partial<TaskSummary> = {}): TaskSummary {
  return {
    id: 't-1',
    state: 'backlog',
    method: 'tasks/send',
    params: { task: 'T', message: '' },
    priority: 'normal',
    progress: 0,
    createdAt: new Date(Date.now() - 10 * days).toISOString(),
    updatedAt: new Date(Date.now() - 10 * days).toISOString(),
    ...overrides,
  }
}

describe('staleTasks', () => {
  test('counts only never-started, untouched backlog/queued tasks', () => {
    const stale = task({ id: 'a' })
    const fresh = task({ id: 'b', updatedAt: new Date().toISOString() })
    const working = task({ id: 'c', state: 'working', progress: 40, updatedAt: new Date(Date.now() - 30 * days).toISOString() })
    const started = task({ id: 'd', state: 'queued', progress: 15 })
    const completed = task({ id: 'e', state: 'completed' })
    expect(staleTaskCount([stale, fresh, working, started, completed])).toBe(1)
  })

  test('never treats an unparseable timestamp as stale', () => {
    expect(isStaleTask(task({ updatedAt: 'not-a-date' }))).toBe(false)
  })

  test('honors the default 7-day window', () => {
    const borderline = task({ updatedAt: new Date(Date.now() - (STALE_RETIRE_DAYS - 1) * days).toISOString() })
    expect(isStaleTask(borderline)).toBe(false)
  })
})
