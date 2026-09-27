import { describe, test, expect, beforeEach } from 'vitest'
import { useBoardStore } from '@app/entities/navigation/model/board.store'

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

  test('groupBy defaults to status', () => {
    expect(useBoardStore.getState().groupBy).toBe('status')
  })

  test('viewMode defaults to board', () => {
    expect(useBoardStore.getState().viewMode).toBe('board')
  })

  test('switching groups clears tasks from the previous group', () => {
    useBoardStore.getState().setSelectedGroupId('group-a')
    useBoardStore
      .getState()
      .setTasks([{ id: '1', state: 'backlog', params: { task: 'A', message: '' } }] as any)

    useBoardStore.getState().setSelectedGroupId('group-b')

    expect(Object.values(useBoardStore.getState().columns).flat()).toEqual([])
  })

  test('rejects stale REST and WebSocket task snapshots', () => {
    const task = (state: string, rowVersion: number) =>
      ({
        id: 'task-1',
        state,
        rowVersion,
        updatedAt: `2026-08-30T00:00:0${rowVersion}Z`,
        params: { task: 'A', message: '' },
      }) as any

    useBoardStore.getState().setTasks([task('blocked', 1)])
    useBoardStore.getState().upsertTask(task('queued', 2))
    useBoardStore.getState().setTasks([task('blocked', 1)])
    useBoardStore.getState().upsertTask(task('blocked', 1))

    expect(useBoardStore.getState().columns.queued[0].rowVersion).toBe(2)
    useBoardStore.getState().upsertTask(task('working', 3))
    expect(useBoardStore.getState().columns.working[0].rowVersion).toBe(3)
  })

  test('uses timestamps across mixed projectors without fabricating a revision', () => {
    const base = {
      id: 'task-1',
      params: { task: 'A', message: '' },
    }
    useBoardStore
      .getState()
      .setTasks([
        { ...base, state: 'queued', rowVersion: 10, updatedAt: '2026-08-30T10:00:00Z' },
      ] as any)
    useBoardStore.getState().upsertTask({
      ...base,
      state: 'working',
      updatedAt: '2026-08-30T10:01:00Z',
    } as any)

    expect(useBoardStore.getState().columns.working[0].rowVersion).toBeUndefined()
    useBoardStore.getState().upsertTask({
      ...base,
      state: 'queued',
      rowVersion: 10,
      updatedAt: '2026-08-30T10:00:00Z',
    } as any)
    expect(useBoardStore.getState().columns.working).toHaveLength(1)
  })

  test('preserves sub-millisecond ordering for rolling-upgrade frames', () => {
    const base = { id: 'task-1', params: { task: 'A', message: '' } }
    useBoardStore.getState().setTasks([
      {
        ...base,
        state: 'working',
        rowVersion: 11,
        updatedAt: '2026-08-30T10:00:00.000900Z',
      },
    ] as any)
    useBoardStore.getState().upsertTask({
      ...base,
      state: 'queued',
      updatedAt: '2026-08-30T10:00:00.000100Z',
    } as any)
    useBoardStore.getState().upsertTask({
      ...base,
      state: 'queued',
      updatedAt: '2026-08-30T10:00:00.000900Z',
    } as any)

    expect(useBoardStore.getState().columns.working).toHaveLength(1)
  })
})
