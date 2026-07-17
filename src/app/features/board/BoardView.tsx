import { DndContext, DragOverlay, type DragEndEvent, type DragStartEvent } from '@dnd-kit/core'
import { useNavigate } from '@tanstack/react-router'
import { ArrowRight, FolderKanban } from 'lucide-react'
import { useState, useEffect, useMemo, useRef, useCallback } from 'react'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { useContextFeaturesStore } from '@app/entities/context/model/context-features.store'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { cn } from '@app/shared/lib/utils'
import { useNavigationStore } from '@app/entities/navigation'
import { KanbanColumn } from './KanbanColumn'
import { TaskCard, taskCardSearchText } from './TaskCard'
import {
  orchestrationApi,
  type ParticipantSummary,
  type TaskSummary,
} from '@app/shared/api/orchestration'
import { InjectionPreviewModal } from '@app/entities/context/ui/InjectionPreviewModal'
import type { ColumnId } from '@app/shared/model/board.types'
import type { ContextPreviewResponse } from '@shared/types/context'
import { AssignmentReadinessPanel, type BoardWorkloadSnapshot } from './AssignmentReadinessPanel'
import { AgentGroupSelector } from './AgentGroupSelector'
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
    selectedGroupId,
    setSelectedGroupId,
    loading,
    error,
    setTasks,
    setLoading,
    setError,
  } = useBoardStore()
  const navigate = useNavigate()
  const selectedProjectId = useNavigationStore((s) => s.selectedProjectId)
  const agentGroups = useNavigationStore((s) => s.agentGroups)
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
    () => ({
      searchQuery,
      priorityFilter,
      assigneeFilter,
      canOpenPublishPreview: canPublishWithContext,
    }),
    [assigneeFilter, canPublishWithContext, priorityFilter, searchQuery]
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
      if (movedDistance <= TAP_DRAG_DISTANCE_PX) {
        void navigate({ to: '/tasks/$taskId', params: { taskId } })
      }
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
    const actionLabel = selectedProjectId ? 'Set up place' : 'Open project settings'
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
              ? 'Set up a place for new tasks before sending work'
              : 'Create or choose a project before creating tasks'}
          </p>
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            {selectedProjectId
              ? 'New tasks need a place before an agent starts them. Open Agents, set up one place, then come back here.'
              : 'Open project settings to create a project, or choose an existing project from the project list. A project keeps tasks, agents, and places together.'}
          </p>
        </div>
        {action ? (
          <button type="button" onClick={action} className={cn(uiStyles.primaryButton, 'px-4')}>
            <span>{actionLabel}</span>
            <ArrowRight size={14} strokeWidth={2.25} aria-hidden="true" />
          </button>
        ) : null}
      </div>
    )
  }

  if (loading) {
    return (
      <BeginnerLoadingState
        title="Checking tasks"
        detail="Forge is checking which tasks are waiting, working, need help, or finished in this project."
        nextStep="If this takes more than a moment, open Tasks again or ask an owner or admin to check the place for new tasks."
        success="Success looks like task columns or an add-the-first-task step."
        testId="board-loading"
        framed={false}
        className="h-full"
      />
    )
  }

  if (error) {
    return (
      <div
        data-testid="board-error"
        className="flex h-full items-center justify-center px-6 text-center"
      >
        <div className="flex max-w-sm flex-col items-center gap-3">
          <p role="alert" aria-live="polite" className="text-ui-body text-apple-red">
            {error}
          </p>
          {selectedGroupId ? (
            <button
              type="button"
              onClick={() => void loadTasksForGroup(selectedGroupId, true)}
              className={cn(
                uiStyles.dangerButton,
                'gap-1.5 border border-apple-red/20 bg-white px-3 dark:bg-white/[0.04]'
              )}
            >
              Check tasks again
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
          taskDestinationSelector={
            <AgentGroupSelector
              groups={agentGroups}
              selectedGroupId={selectedGroupId}
              selectedProjectId={selectedProjectId}
              onSelectGroup={setSelectedGroupId}
            />
          }
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
            aria-live="polite"
            className={cn(uiStyles.error, 'mb-0')}
          >
            {actionError}
          </div>
        ) : null}
        {hasActiveBoardFilter && filterCounts.visible === 0 ? (
          <div
            data-testid="board-filter-empty"
            role="status"
            aria-live="polite"
            className="flex min-h-64 flex-1 flex-col items-center justify-center gap-3 rounded-card border border-dashed border-black/10 px-6 text-center dark:border-white/10"
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
            <button type="button" onClick={clearBoardFilters} className={uiStyles.secondaryButton}>
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
                onTaskClick={(taskId) =>
                  void navigate({ to: '/tasks/$taskId', params: { taskId } })
                }
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
      title: 'Search is hiding tasks',
      detail: 'Tasks may still exist. Show all tasks, then search with fewer words.',
      nextStep: 'Next: show all tasks before deciding the board is empty.',
    }
  }

  if (!hasSearch && hasPriority && !hasAssignee) {
    return {
      title: 'Priority choice is hiding tasks',
      detail: 'Tasks may still exist. Show all tasks, then choose one priority at a time.',
      nextStep: 'Next: show all tasks before deciding this priority is empty.',
    }
  }

  if (!hasSearch && !hasPriority && hasAssignee) {
    return {
      title: 'Agent choice is hiding tasks',
      detail: 'Tasks may still exist. Show all tasks, then choose one agent option at a time.',
      nextStep: 'Next: show all tasks before deciding nothing is waiting.',
    }
  }

  return {
    title: 'Search and choices are hiding tasks',
    detail: 'The board still has tasks. Show all tasks, then narrow one choice at a time.',
    nextStep: 'Next: show all tasks, then narrow the board one choice at a time.',
  }
}

interface BoardFilters {
  searchQuery: string
  priorityFilter: BoardPriorityFilter
  assigneeFilter: BoardAssigneeFilter
  canOpenPublishPreview: boolean
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

  return taskCardSearchText(task, {
    canOpenPublishPreview: filters.canOpenPublishPreview,
  }).includes(query)
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
