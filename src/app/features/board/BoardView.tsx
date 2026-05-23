import { DndContext, DragOverlay, type DragEndEvent, type DragStartEvent } from '@dnd-kit/core'
import { useState, useEffect } from 'react'
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
import { AssignmentReadinessPanel } from './AssignmentReadinessPanel'

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

export function BoardView() {
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
  const [activeTask, setActiveTask] = useState<TaskSummary | null>(null)
  const [previewTask, setPreviewTask] = useState<TaskSummary | null>(null)
  const [preview, setPreview] = useState<ContextPreviewResponse | null>(null)
  const [previewLoading, setPreviewLoading] = useState(false)
  const [previewError, setPreviewError] = useState<string | null>(null)
  const [publishing, setPublishing] = useState(false)
  const [participants, setParticipants] = useState<ParticipantSummary[]>([])
  const [participantsLoading, setParticipantsLoading] = useState(false)
  const [participantsError, setParticipantsError] = useState<string | null>(null)

  useEffect(() => {
    if (!selectedGroupId) return
    const groupId = selectedGroupId
    let cancelled = false
    async function loadTasks(showLoading: boolean) {
      try {
        if (showLoading) setLoading(true)
        setError(null)
        const tasks = await orchestrationApi.getTasks(groupId)
        if (!cancelled) setTasks(tasks)
      } catch (err) {
        if (!cancelled && showLoading) {
          setError(err instanceof Error ? err.message : 'Failed to load tasks')
        }
      } finally {
        if (!cancelled && showLoading) setLoading(false)
      }
    }
    void loadTasks(true)
    const fallbackRefresh = window.setInterval(() => {
      if (document.visibilityState === 'hidden') return
      void loadTasks(false)
    }, BOARD_FALLBACK_REFRESH_MS)
    return () => {
      cancelled = true
      window.clearInterval(fallbackRefresh)
    }
  }, [selectedGroupId, setTasks, setLoading, setError])

  async function loadParticipants(showLoading = true) {
    try {
      if (showLoading) setParticipantsLoading(true)
      setParticipantsError(null)
      setParticipants(await orchestrationApi.getParticipants('all'))
    } catch (err) {
      setParticipants([])
      setParticipantsError(err instanceof Error ? err.message : 'Failed to load agent readiness')
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
    moveTask(taskId, colId)

    try {
      await orchestrationApi.updateTask(taskId, { state: newState })
    } catch {
      // Rollback on failure
      if (previousCol) moveTask(taskId, previousCol)
      console.error('Failed to persist task move')
    }
  }

  async function handleQuickCreate(title: string) {
    if (!selectedGroupId) return
    try {
      const response = await orchestrationApi.createTask({
        groupId: selectedGroupId,
        params: { task: title, message: '' },
      })
      if (response.ok && response.task) {
        upsertTask(response.task)
      }
    } catch (err) {
      console.error('Failed to create task:', err)
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
      setPreviewError(err instanceof Error ? err.message : 'Failed to load context preview')
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
      setPreviewError(err instanceof Error ? err.message : 'Failed to publish task')
    } finally {
      setPublishing(false)
    }
  }

  if (!selectedGroupId) {
    return (
      <div
        data-testid="board-no-group"
        className="mx-auto flex h-full max-w-sm flex-col items-center justify-center gap-4 px-6 text-center"
      >
        <div className="flex h-14 w-14 items-center justify-center rounded-full bg-apple-blue/10 text-apple-blue">
          <svg
            width="26"
            height="26"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.75"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M3 7V5a2 2 0 0 1 2-2h2" />
            <path d="M17 3h2a2 2 0 0 1 2 2v2" />
            <path d="M21 17v2a2 2 0 0 1-2 2h-2" />
            <path d="M7 21H5a2 2 0 0 1-2-2v-2" />
            <rect width="7" height="5" x="7" y="7" rx="1" />
            <rect width="7" height="5" x="7" y="12" rx="1" />
          </svg>
        </div>
        <div className="space-y-1">
          <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {selectedProjectId
              ? 'No task group in this project yet'
              : 'Pick a project to get started'}
          </p>
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            {selectedProjectId
              ? 'Create or select a task group first. Agents in a group can receive tasks from the board.'
              : 'Tasks route through project-scoped task groups. Choose a project from the sidebar, or create your first project in Settings → Projects.'}
          </p>
        </div>
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
        className="flex h-full items-center justify-center text-ui-body text-apple-red"
      >
        {error}
      </div>
    )
  }

  return (
    <DndContext onDragStart={handleDragStart} onDragEnd={handleDragEnd}>
      <div className="flex h-full flex-col gap-3 p-1">
        <AssignmentReadinessPanel
          participants={participants}
          loading={participantsLoading}
          error={participantsError}
          onRefresh={() => void loadParticipants(true)}
        />
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto md:flex-row md:overflow-x-auto md:overflow-y-hidden">
          {COLUMN_ORDER.map((colId) => (
            <KanbanColumn
              key={colId}
              columnId={colId}
              tasks={columns[colId]}
              onTaskClick={setSelectedTask}
              onTaskPublish={
                canPublishWithContext ? (task) => void openPublishPreview(task) : undefined
              }
              onQuickCreate={handleQuickCreate}
            />
          ))}
        </div>
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
