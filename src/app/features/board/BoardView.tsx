import { DndContext, DragOverlay, type DragEndEvent, type DragStartEvent } from '@dnd-kit/core'
import { ArrowRight, FolderKanban } from 'lucide-react'
import { useState, useEffect, useMemo, useRef, useCallback } from 'react'
import { useBoardStore } from '@app/shared/model/board.store'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { useNavigationStore } from '@app/entities/navigation'
import { KanbanColumn } from './KanbanColumn'
import { TaskCard } from './TaskCard'
import {
  orchestrationApi,
  type ParticipantSummary,
  type TaskSummary,
} from '@app/shared/api/orchestration'
import { InjectionPreviewModal } from '@app/entities/context/ui/InjectionPreviewModal'
import type { ColumnId } from '@app/shared/model/board.types'
import type { ContextPreviewResponse } from '@shared/types/context'
import { AssignmentReadinessPanel, type BoardWorkloadSnapshot } from './AssignmentReadinessPanel'
import {
  BoardToolbar,
  type BoardAssigneeFilter,
  type BoardDisplayMode,
  type BoardFilterCounts,
  type BoardPriorityFilter,
} from './BoardToolbar'
import { boardActionErrorMessage } from './boardErrorMessages'
import { useWebSocket } from '@app/shared/model/websocket.context'

const COLUMN_ORDER: ColumnId[] = [
  'backlog',
  'queued',
  'working',
  'blocked',
  'done',
  'failed',
  'canceled',
]
const BOARD_FALLBACK_REFRESH_MS = 30_000
const TAP_DRAG_DISTANCE_PX = 6

interface BoardFilterEmptyCopy {
  title: string
  detail: string
  nextStep: string
}

interface BoardViewProps {
  onOpenProjectsSetup?: () => void
  onOpenTaskQueues?: () => void
}

export function BoardView({ onOpenProjectsSetup, onOpenTaskQueues }: BoardViewProps = {}) {
  const {
    columns,
    moveTask,
    upsertTask,
    setSelectedTask,
    selectedGroupId,
    loading,
    error,
    setTasks,
    setLoading,
    setError,
  } = useBoardStore()
  const selectedProjectId = useNavigationStore((s) => s.selectedProjectId)
  const canPublishWithContext = useContextFeaturesStore((s) => s.preview && s.injection)
  const { status: wsStatus } = useWebSocket()
  const wsStatusRef = useRef(wsStatus)
  const [activeTask, setActiveTask] = useState<TaskSummary | null>(null)
  const [previewTask, setPreviewTask] = useState<TaskSummary | null>(null)
  const [preview, setPreview] = useState<ContextPreviewResponse | null>(null)
  const [previewLoading, setPreviewLoading] = useState(false)
  const [previewError, setPreviewError] = useState<string | null>(null)
  const [publishing, setPublishing] = useState(false)
  const [participants, setParticipants] = useState<ParticipantSummary[]>([])
  const [participantsLoading, setParticipantsLoading] = useState(false)
  const [participantsError, setParticipantsError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [priorityFilter, setPriorityFilter] = useState<BoardPriorityFilter>('all')
  const [assigneeFilter, setAssigneeFilter] = useState<BoardAssigneeFilter>('all')
  const [displayMode, setDisplayMode] = useState<BoardDisplayMode>('comfortable')
  const workload = useMemo(() => summarizeWorkload(columns), [columns])
  const boardFilters = useMemo(
    () => ({ searchQuery, priorityFilter, assigneeFilter }),
    [assigneeFilter, priorityFilter, searchQuery]
  )
  const visibleColumns = useMemo(
    () => filterBoardColumns(columns, boardFilters),
    [columns, boardFilters]
  )
  const filterCounts = useMemo(
    () => summarizeBoardFilters(columns, visibleColumns),
    [columns, visibleColumns]
  )
  const boardFilterEmpty = useMemo(() => boardFilterEmptyCopy(boardFilters), [boardFilters])
  const hasActiveBoardFilter =
    searchQuery.trim().length > 0 || priorityFilter !== 'all' || assigneeFilter !== 'all'
  const clearBoardFilters = () => {
    setSearchQuery('')
    setPriorityFilter('all')
    setAssigneeFilter('all')
  }
  const loadTasksForGroup = useCallback(
    async (groupId: string, showLoading: boolean, shouldApply: () => boolean = () => true) => {
      try {
        if (showLoading && shouldApply()) setLoading(true)
        if (shouldApply()) setError(null)
        const tasks = await orchestrationApi.getTasks(groupId)
        if (shouldApply()) setTasks(tasks)
      } catch (err) {
        if (showLoading && shouldApply()) {
          setError(boardActionErrorMessage('loadTasks', err))
        }
      } finally {
        if (showLoading && shouldApply()) setLoading(false)
      }
    },
    [setError, setLoading, setTasks]
  )

  useEffect(() => {
    wsStatusRef.current = wsStatus
  }, [wsStatus])

  useEffect(() => {
    if (!selectedGroupId) return
    const groupId = selectedGroupId
    let cancelled = false
    const shouldApply = () => !cancelled
    void loadTasksForGroup(groupId, true, shouldApply)
    const fallbackRefresh = window.setInterval(() => {
      if (document.visibilityState === 'hidden') return
      if (wsStatusRef.current === 'connected') return
      void loadTasksForGroup(groupId, false, shouldApply)
    }, BOARD_FALLBACK_REFRESH_MS)
    return () => {
      cancelled = true
      window.clearInterval(fallbackRefresh)
    }
  }, [loadTasksForGroup, selectedGroupId])

  async function loadParticipants(showLoading = true) {
    try {
      if (showLoading) setParticipantsLoading(true)
      setParticipantsError(null)
      setParticipants(await orchestrationApi.getParticipants('all'))
    } catch (err) {
      setParticipants([])
      setParticipantsError(boardActionErrorMessage('loadReadiness', err))
    } finally {
      if (showLoading) setParticipantsLoading(false)
    }
  }

  useEffect(() => {
    if (!selectedGroupId) {
      setParticipants([])
      return
    }
    void loadParticipants(true)
    const fallbackRefresh = window.setInterval(() => {
      if (document.visibilityState === 'hidden') return
      if (wsStatusRef.current === 'connected') return
      void loadParticipants(false)
    }, BOARD_FALLBACK_REFRESH_MS)
    return () => window.clearInterval(fallbackRefresh)
  }, [selectedGroupId])

  function handleDragStart(event: DragStartEvent) {
    const taskId = event.active.id as string
    for (const col of Object.values(columns)) {
      const task = col.find((t) => t.id === taskId)
      if (task) {
        setActiveTask(task)
        break
      }
    }
  }

  async function handleDragEnd(event: DragEndEvent) {
    setActiveTask(null)
    const { active, over } = event
    if (!over) return

    const taskId = active.id as string
    const targetColumn = over.id as string

    if (!COLUMN_ORDER.includes(targetColumn as ColumnId)) return

    const colId = targetColumn as ColumnId

    // Skip if task is already in the target column
    const currentCol = COLUMN_ORDER.find((c) => columns[c].some((t) => t.id === taskId))
    if (currentCol === colId) {
      const movedDistance = Math.hypot(event.delta.x, event.delta.y)
      if (movedDistance <= TAP_DRAG_DISTANCE_PX) setSelectedTask(taskId)
      return
    }

    const newState = colId === 'done' ? 'completed' : colId
    const previousCol = currentCol

    // Optimistic update
    setActionError(null)
    moveTask(taskId, colId)

    try {
      await orchestrationApi.updateTask(taskId, { state: newState })
    } catch (err) {
      // Rollback on failure
      if (previousCol) moveTask(taskId, previousCol)
      setActionError(boardActionErrorMessage('moveTask', err))
      console.error('Failed to persist task move')
    }
  }

  async function handleQuickCreate(title: string): Promise<boolean | string> {
    if (!selectedGroupId) return false
    setActionError(null)
    try {
      const response = await orchestrationApi.createTask({
        groupId: selectedGroupId,
        params: { task: title, message: '' },
      })
      if (response.ok && response.task) {
        upsertTask(response.task)
        return true
      } else {
        const message = boardActionErrorMessage('createTask', response)
        setActionError(message)
        return message
      }
    } catch (err) {
      const message = boardActionErrorMessage('createTask', err)
      setActionError(message)
      console.error('Failed to create task:', err)
      return message
    }
  }

  async function openPublishPreview(task: TaskSummary) {
    setPreviewTask(task)
    setPreview(null)
    setPreviewError(null)
    setPreviewLoading(true)
    try {
      const participants = await orchestrationApi.getParticipants('available')
      const agentId = participants[0]?.agentId
      if (!agentId) {
        throw new Error('No available agent for context preview')
      }
      setPreview(await orchestrationApi.previewContext(task.id, agentId))
    } catch (err) {
      setPreviewError(boardActionErrorMessage('previewContext', err))
    } finally {
      setPreviewLoading(false)
    }
  }

  async function publishPreview(selection: { pinnedIds: string[]; removedIds: string[] }) {
    if (!previewTask || !preview) return
    setPublishing(true)
    setPreviewError(null)
    try {
      const response = await orchestrationApi.publishWithContext(previewTask.id, {
        contextPreviewId: preview.contextPreviewId,
        previewHash: preview.previewHash,
        pinnedIds: selection.pinnedIds,
        removedIds: selection.removedIds,
      })
      if (response.ok && response.task) upsertTask(response.task)
      setPreviewTask(null)
      setPreview(null)
    } catch (err) {
      setPreviewError(boardActionErrorMessage('publishTask', err))
    } finally {
      setPublishing(false)
    }
  }

  if (!selectedGroupId) {
    const actionLabel = selectedProjectId ? 'Set up where tasks wait' : 'Open project settings'
    const action = selectedProjectId ? onOpenTaskQueues : onOpenProjectsSetup

    return (
      <div
        data-testid="board-no-group"
        className="mx-auto flex h-full max-w-sm flex-col items-center justify-center gap-4 px-6 text-center"
      >
        <div className="flex h-14 w-14 items-center justify-center rounded-full bg-apple-blue/10 text-apple-blue">
          <FolderKanban size={26} strokeWidth={1.85} aria-hidden="true" />
        </div>
        <div className="space-y-1">
          <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {selectedProjectId
              ? 'Set up where tasks wait before sending work'
              : 'Create or choose a project before creating tasks'}
          </p>
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            {selectedProjectId
              ? 'New tasks need a place to wait before an agent starts them. Set that up, then come back here.'
              : 'Open project settings to create a project, or choose an existing project from the project list. A project keeps tasks, agents, and task queues together.'}
          </p>
        </div>
        {action ? (
          <button
            type="button"
            onClick={action}
            className="inline-flex h-9 items-center justify-center gap-1.5 rounded-full bg-apple-blue px-4 text-ui-button font-semibold text-white transition-colors hover:bg-apple-blue-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
          >
            <span>{actionLabel}</span>
            <ArrowRight size={14} strokeWidth={2.25} aria-hidden="true" />
          </button>
        ) : null}
      </div>
    )
  }

  if (loading) {
    return (
      <div
        data-testid="board-loading"
        className="flex h-full animate-pulse items-center justify-center text-ui-body text-secondary-light dark:text-secondary-dark"
      >
        Loading tasks…
      </div>
    )
  }

  if (error) {
    return (
      <div
        data-testid="board-error"
        className="flex h-full items-center justify-center px-6 text-center"
      >
        <div className="flex max-w-sm flex-col items-center gap-3">
          <p className="text-ui-body text-apple-red">{error}</p>
          {selectedGroupId ? (
            <button
              type="button"
              onClick={() => void loadTasksForGroup(selectedGroupId, true)}
              className="inline-flex h-9 items-center justify-center gap-1.5 rounded-full border border-apple-red/20 bg-white px-3 text-ui-button font-medium text-apple-red transition-colors hover:bg-apple-red/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/30 dark:bg-white/[0.04]"
            >
              Try Again
            </button>
          ) : null}
        </div>
      </div>
    )
  }

  return (
    <DndContext onDragStart={handleDragStart} onDragEnd={handleDragEnd}>
      <div className="flex h-full flex-col gap-3 p-1">
        <AssignmentReadinessPanel
          participants={participants}
          workload={workload}
          loading={participantsLoading}
          error={participantsError}
          onRefresh={() => void loadParticipants(true)}
        />
        <BoardToolbar
          searchQuery={searchQuery}
          onSearchQueryChange={setSearchQuery}
          priorityFilter={priorityFilter}
          onPriorityFilterChange={setPriorityFilter}
          assigneeFilter={assigneeFilter}
          onAssigneeFilterChange={setAssigneeFilter}
          displayMode={displayMode}
          onDisplayModeChange={setDisplayMode}
          counts={filterCounts}
          onClear={clearBoardFilters}
        />
        {actionError ? (
          <div
            data-testid="board-action-error"
            role="alert"
            className="rounded-lg border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red"
          >
            {actionError}
          </div>
        ) : null}
        {hasActiveBoardFilter && filterCounts.visible === 0 ? (
          <div
            data-testid="board-filter-empty"
            className="flex min-h-64 flex-1 flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-black/10 px-6 text-center dark:border-white/10"
          >
            <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              {boardFilterEmpty.title}
            </p>
            <p className="max-w-sm text-ui-body text-secondary-light dark:text-secondary-dark">
              {boardFilterEmpty.detail}
            </p>
            <p className="max-w-sm text-ui-body text-secondary-light dark:text-secondary-dark">
              {boardFilterEmpty.nextStep}
            </p>
            <button
              type="button"
              onClick={clearBoardFilters}
              className="inline-flex h-9 items-center justify-center rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:border-apple-blue/35 hover:text-apple-blue focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 dark:border-white/[0.1] dark:bg-[#2a2a2c] dark:text-foreground-dark"
            >
              Show all tasks
            </button>
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto md:flex-row md:overflow-x-auto md:overflow-y-hidden">
            {COLUMN_ORDER.map((colId) => (
              <KanbanColumn
                key={colId}
                columnId={colId}
                tasks={visibleColumns[colId]}
                onTaskClick={setSelectedTask}
                onTaskPublish={
                  canPublishWithContext ? (task) => void openPublishPreview(task) : undefined
                }
                onQuickCreate={handleQuickCreate}
                displayMode={displayMode}
              />
            ))}
          </div>
        )}
      </div>
      <DragOverlay>{activeTask ? <TaskCard task={activeTask} /> : null}</DragOverlay>
      <InjectionPreviewModal
        isOpen={previewTask !== null}
        preview={preview}
        loading={previewLoading}
        publishing={publishing}
        error={previewError}
        onClose={() => {
          if (!publishing) {
            setPreviewTask(null)
            setPreview(null)
          }
        }}
        onConfirm={(selection) => void publishPreview(selection)}
      />
    </DndContext>
  )
}

function boardFilterEmptyCopy(filters: BoardFilters): BoardFilterEmptyCopy {
  const hasSearch = filters.searchQuery.trim().length > 0
  const hasPriority = filters.priorityFilter !== 'all'
  const hasAssignee = filters.assigneeFilter !== 'all'

  if (hasSearch && !hasPriority && !hasAssignee) {
    return {
      title: 'Search is hiding every task',
      detail: 'Tasks may still exist, but none match the words you typed.',
      nextStep: 'Next: show all tasks before assuming the board is empty.',
    }
  }

  if (!hasSearch && hasPriority && !hasAssignee) {
    return {
      title: 'This priority filter hides every task',
      detail: 'Tasks may still exist at another priority level.',
      nextStep: 'Next: show all tasks to review the full board.',
    }
  }

  if (!hasSearch && !hasPriority && hasAssignee) {
    return {
      title: 'This agent filter hides every task',
      detail: 'Tasks may still exist with a different agent status.',
      nextStep: 'Next: show all tasks before deciding nothing is waiting.',
    }
  }

  return {
    title: 'Filters are hiding every task',
    detail: 'The board still has tasks, but the current search and filters hide all of them.',
    nextStep: 'Next: show all tasks, then narrow the board one filter at a time.',
  }
}

interface BoardFilters {
  searchQuery: string
  priorityFilter: BoardPriorityFilter
  assigneeFilter: BoardAssigneeFilter
}

function summarizeWorkload(columns: Record<ColumnId, TaskSummary[]>): BoardWorkloadSnapshot {
  const backlog = columns.backlog.length
  return {
    backlog,
    unassigned: columns.backlog.filter((task) => !task.assignedTo && !task.assignedAgentName)
      .length,
    inFlight: columns.queued.length + columns.working.length,
    blocked: columns.blocked.length,
    review: columns.done.length,
  }
}

function filterBoardColumns(
  columns: Record<ColumnId, TaskSummary[]>,
  filters: BoardFilters
): Record<ColumnId, TaskSummary[]> {
  return COLUMN_ORDER.reduce(
    (result, columnId) => {
      result[columnId] = columns[columnId].filter((task) => taskMatchesBoardFilters(task, filters))
      return result
    },
    {} as Record<ColumnId, TaskSummary[]>
  )
}

function taskMatchesBoardFilters(task: TaskSummary, filters: BoardFilters): boolean {
  if (filters.priorityFilter !== 'all' && task.priority !== filters.priorityFilter) return false
  if (filters.assigneeFilter === 'assigned' && !task.assignedTo && !task.assignedAgentName)
    return false
  if (filters.assigneeFilter === 'unassigned' && (task.assignedTo || task.assignedAgentName))
    return false

  const query = filters.searchQuery.trim().toLowerCase()
  if (!query) return true

  return [
    task.id,
    task.params.task,
    task.params.message,
    task.assignedAgentName,
    task.priority,
    task.blockedHint,
    task.error,
  ]
    .filter(Boolean)
    .some((value) => String(value).toLowerCase().includes(query))
}

function summarizeBoardFilters(
  columns: Record<ColumnId, TaskSummary[]>,
  visibleColumns: Record<ColumnId, TaskSummary[]>
): BoardFilterCounts {
  const tasks = COLUMN_ORDER.flatMap((columnId) => columns[columnId])
  const visibleTasks = COLUMN_ORDER.flatMap((columnId) => visibleColumns[columnId])
  return {
    total: tasks.length,
    visible: visibleTasks.length,
    priority: {
      all: tasks.length,
      urgent: tasks.filter((task) => task.priority === 'urgent').length,
      high: tasks.filter((task) => task.priority === 'high').length,
      normal: tasks.filter((task) => task.priority === 'normal').length,
      low: tasks.filter((task) => task.priority === 'low').length,
    },
    assignee: {
      all: tasks.length,
      assigned: tasks.filter((task) => task.assignedTo || task.assignedAgentName).length,
      unassigned: tasks.filter((task) => !task.assignedTo && !task.assignedAgentName).length,
    },
  }
}
