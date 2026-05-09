import { describe, test, expect, beforeEach } from 'vitest'
import { useBoardStore } from '@app/shared/model/board.store'

beforeEach(() => {
  useBoardStore.getState().reset()
})

describe('Board Store', () => {
  test('initializes with empty columns', () => {
    const state = useBoardStore.getState()
    expect(state.columns).toEqual({
      backlog: [],
      queued: [],
      working: [],
      blocked: [],
      done: [],
      failed: [],
      canceled: [],
    })
  })

  test('setTasks distributes tasks to correct columns', () => {
    useBoardStore.getState().setTasks([
      { id: '1', state: 'backlog', params: { task: 'A', message: '' } },
      { id: '2', state: 'working', params: { task: 'B', message: '' } },
      { id: '3', state: 'completed', params: { task: 'C', message: '' } },
      { id: '4', state: 'failed', params: { task: 'D', message: '' } },
      { id: '5', state: 'canceled', params: { task: 'E', message: '' } },
    ] as any)
    const { columns } = useBoardStore.getState()
    expect(columns.backlog).toHaveLength(1)
    expect(columns.working).toHaveLength(1)
    expect(columns.done).toHaveLength(1)
    expect(columns.failed).toHaveLength(1)
    expect(columns.canceled).toHaveLength(1)
  })

  test('moveTask changes task state and column', () => {
    useBoardStore
      .getState()
      .setTasks([{ id: '1', state: 'backlog', params: { task: 'A', message: '' } }] as any)
    useBoardStore.getState().moveTask('1', 'queued')
    const { columns } = useBoardStore.getState()
    expect(columns.backlog).toHaveLength(0)
    expect(columns.queued).toHaveLength(1)
    expect(columns.queued[0].id).toBe('1')
  })

  test('groupBy defaults to status', () => {
    expect(useBoardStore.getState().groupBy).toBe('status')
  })

  test('viewMode defaults to board', () => {
    expect(useBoardStore.getState().viewMode).toBe('board')
  })
})
