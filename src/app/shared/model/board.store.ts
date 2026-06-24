import { create } from 'zustand'
import type { TaskContextCounts, TaskSummary } from '@app/shared/api/orchestration'
import type { ColumnId, ViewMode, GroupBy } from '@app/shared/model/board.types'

interface BoardState {
  columns: Record<ColumnId, TaskSummary[]>
  viewMode: ViewMode
  groupBy: GroupBy
  selectedTaskId: string | null
  selectedGroupId: string | null
  loading: boolean
  error: string | null
  setTasks: (tasks: TaskSummary[]) => void
  moveTask: (taskId: string, toColumn: ColumnId) => void
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
  setSelectedTask: (id: string | null) => void
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
  selectedTaskId: null as string | null,
  selectedGroupId: null as string | null,
  loading: false,
  error: null as string | null,
}

export const useBoardStore = create<BoardState>((set, get) => ({
  ...initialState,
  setTasks: (tasks) => set({ columns: distributeToColumns(tasks) }),
  moveTask: (taskId, toColumn) => {
    const { columns } = get()
    let task: TaskSummary | undefined
    const newColumns = { ...columns }
    for (const col of Object.keys(newColumns) as ColumnId[]) {
      const idx = newColumns[col].findIndex((t) => t.id === taskId)
      if (idx !== -1) {
        task = newColumns[col][idx]
        newColumns[col] = [...newColumns[col].slice(0, idx), ...newColumns[col].slice(idx + 1)]
        break
      }
    }
    if (task) {
      const updatedTask = {
        ...task,
        state: toColumn === 'done' ? 'completed' : toColumn,
      } as TaskSummary
      newColumns[toColumn] = [...newColumns[toColumn], updatedTask]
      set({ columns: newColumns })
    }
  },
  upsertTask: (task) => {
    const { columns } = get()
    const newColumns = { ...columns }
    const targetCol = stateToColumn(task.state)
    for (const col of Object.keys(newColumns) as ColumnId[]) {
      newColumns[col] = newColumns[col].filter((t) => t.id !== task.id)
    }
    newColumns[targetCol] = [...newColumns[targetCol], task]
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
  setSelectedTask: (selectedTaskId) => set({ selectedTaskId }),
  setSelectedGroupId: (selectedGroupId) => set({ selectedGroupId }),
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
