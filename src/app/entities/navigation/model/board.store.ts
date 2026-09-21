import { create } from 'zustand'
import type { TaskContextCounts, TaskSummary } from '@app/shared/api/orchestration'
import type { ColumnId, ViewMode, GroupBy } from '@app/shared/model/board.types'

interface BoardState {
  columns: Record<ColumnId, TaskSummary[]>
  viewMode: ViewMode
  groupBy: GroupBy
  selectedGroupId: string | null
  loading: boolean
  error: string | null
  setTasks: (tasks: TaskSummary[]) => void
  clearTasks: () => void
  upsertTask: (task: TaskSummary) => void
  updateTaskContextCounts: (
    taskId: string,
    contextCounts: TaskContextCounts,
    options?: { groupId?: string }
  ) => void
  incrementTaskContextCounts: (
    taskId: string,
    itemKind: 'memory' | 'skill',
    options?: { groupId?: string }
  ) => void
  setViewMode: (mode: ViewMode) => void
  setGroupBy: (group: GroupBy) => void
  setSelectedGroupId: (id: string | null) => void
  setLoading: (loading: boolean) => void
  setError: (error: string | null) => void
  reset: () => void
}

function stateToColumn(state: string): ColumnId {
  switch (state) {
    case 'backlog':
      return 'backlog'
    case 'queued':
      return 'queued'
    case 'working':
      return 'working'
    case 'blocked':
      return 'blocked'
    case 'completed':
      return 'done'
    case 'failed':
      return 'failed'
    case 'canceled':
      return 'canceled'
    default:
      return 'backlog'
  }
}

function distributeToColumns(tasks: TaskSummary[]): Record<ColumnId, TaskSummary[]> {
  const columns: Record<ColumnId, TaskSummary[]> = {
    backlog: [],
    queued: [],
    working: [],
    blocked: [],
    done: [],
    failed: [],
    canceled: [],
  }
  for (const task of tasks) columns[stateToColumn(task.state)].push(task)
  return columns
}

function isOlderTask(incoming: TaskSummary, current: TaskSummary): boolean {
  if (
    incoming.rowVersion !== undefined &&
    current.rowVersion !== undefined &&
    incoming.rowVersion !== current.rowVersion
  ) {
    return incoming.rowVersion < current.rowVersion
  }
  const timestampOrder = compareRfc3339(incoming.updatedAt, current.updatedAt)
  if (timestampOrder !== 0) return timestampOrder < 0
  return incoming.rowVersion === undefined && current.rowVersion !== undefined
}

function compareRfc3339(left: string, right: string): number {
  const leftNanos = rfc3339Nanos(left)
  const rightNanos = rfc3339Nanos(right)
  if (leftNanos === rightNanos) return 0
  return leftNanos < rightNanos ? -1 : 1
}

function rfc3339Nanos(value: string): bigint {
  const milliseconds = Date.parse(value)
  if (!Number.isFinite(milliseconds)) return 0n
  const fraction = value.match(/:\d{2}(?:\.(\d{1,9}))?(?:Z|[+-]\d{2}:\d{2})$/)?.[1] ?? ''
  return BigInt(Math.floor(milliseconds / 1000)) * 1_000_000_000n + BigInt(fraction.padEnd(9, '0'))
}

function currentTasks(columns: Record<ColumnId, TaskSummary[]>): TaskSummary[] {
  return Object.values(columns).flat()
}

const initialState = {
  columns: {
    backlog: [],
    queued: [],
    working: [],
    blocked: [],
    done: [],
    failed: [],
    canceled: [],
  } as Record<ColumnId, TaskSummary[]>,
  viewMode: 'board' as ViewMode,
  groupBy: 'status' as GroupBy,
  selectedGroupId: null as string | null,
  loading: false,
  error: null as string | null,
}

export const useBoardStore = create<BoardState>((set, get) => ({
  ...initialState,
  setTasks: (tasks) =>
    set((state) => {
      const newest = new Map(currentTasks(state.columns).map((task) => [task.id, task]))
      for (const task of tasks) {
        const current = newest.get(task.id)
        if (!current || !isOlderTask(task, current)) newest.set(task.id, task)
      }
      return { columns: distributeToColumns([...newest.values()]) }
    }),
  clearTasks: () => set({ columns: distributeToColumns([]) }),
  upsertTask: (task) => {
    const { columns } = get()
    const existing = currentTasks(columns).find((item) => item.id === task.id)
    if (existing && isOlderTask(task, existing)) return
    const newColumns = { ...columns }
    const targetCol = stateToColumn(task.state)
    for (const col of Object.keys(newColumns) as ColumnId[]) {
      newColumns[col] = newColumns[col].filter((t) => t.id !== task.id)
    }
    // Realtime task_update frames omit the wait prediction; keep the last
    // known estimate while the task stays in the same waiting state.
    const merged =
      existing?.state === 'queued' &&
      task.state === 'queued' &&
      task.waitEstimate === undefined &&
      existing.waitEstimate
        ? { ...task, waitEstimate: existing.waitEstimate }
        : task
    newColumns[targetCol] = [...newColumns[targetCol], merged]
    set({ columns: newColumns })
  },
  updateTaskContextCounts: (taskId, contextCounts, options) => {
    const { columns, selectedGroupId } = get()
    if (selectedGroupId && options?.groupId && options.groupId !== selectedGroupId) return

    let found = false
    const newColumns = { ...columns }
    for (const col of Object.keys(newColumns) as ColumnId[]) {
      newColumns[col] = newColumns[col].map((task) => {
        if (task.id !== taskId) return task
        found = true
        return { ...task, contextCounts: normalizeContextCounts(contextCounts) }
      })
    }

    if (found) set({ columns: newColumns })
  },
  incrementTaskContextCounts: (taskId, itemKind, options) => {
    const { columns, selectedGroupId } = get()
    if (selectedGroupId && options?.groupId && options.groupId !== selectedGroupId) return

    let found = false
    const newColumns = { ...columns }
    for (const col of Object.keys(newColumns) as ColumnId[]) {
      newColumns[col] = newColumns[col].map((task) => {
        if (task.id !== taskId) return task
        found = true
        const current = normalizeContextCounts(task.contextCounts)
        const appliedMemories = current.appliedMemories + (itemKind === 'memory' ? 1 : 0)
        const appliedSkills = current.appliedSkills + (itemKind === 'skill' ? 1 : 0)
        return {
          ...task,
          contextCounts: {
            appliedMemories,
            appliedSkills,
            total: appliedMemories + appliedSkills,
          },
        }
      })
    }

    if (found) set({ columns: newColumns })
  },
  setViewMode: (viewMode) => set({ viewMode }),
  setGroupBy: (groupBy) => set({ groupBy }),
  setSelectedGroupId: (selectedGroupId) =>
    set((state) => ({
      selectedGroupId,
      columns: state.selectedGroupId === selectedGroupId ? state.columns : distributeToColumns([]),
    })),
  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),
  reset: () => set(initialState),
}))

function normalizeContextCounts(counts?: Partial<TaskContextCounts> | null): TaskContextCounts {
  const appliedMemories = nonNegativeCount(counts?.appliedMemories)
  const appliedSkills = nonNegativeCount(counts?.appliedSkills)
  return {
    appliedMemories,
    appliedSkills,
    total: nonNegativeCount(counts?.total ?? appliedMemories + appliedSkills),
  }
}

function nonNegativeCount(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 0
  return Math.max(0, Math.trunc(value))
}
