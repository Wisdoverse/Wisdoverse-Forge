import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { orchestrationApi, type TaskSummary } from '@app/shared/api/orchestration'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import { uiStyles } from '@app/shared/lib/uiStyles'

interface TaskDocumentPageProps {
  taskId: string
}

export function TaskDocumentPage({ taskId }: TaskDocumentPageProps) {
  const navigate = useNavigate()
  const columns = useBoardStore((s) => s.columns)
  const upsertTask = useBoardStore((s) => s.upsertTask)
  const storeTask = useMemo(
    () =>
      Object.values(columns)
        .flat()
        .find((t) => t.id === taskId) ?? null,
    [columns, taskId]
  )
  const [fetchState, setFetchState] = useState<'idle' | 'loading' | 'missing' | 'failed'>('idle')
  const titleRef = useRef<HTMLHeadingElement>(null)

  // Cold deep link: the board store only fills when BoardView mounts, so
  // fetch this one task directly (GET /orchestration/tasks/{id}) and upsert it.
  useEffect(() => {
    if (storeTask) return
    let cancelled = false
    setFetchState('loading')
    orchestrationApi
      .getTask(taskId)
      .then((task: TaskSummary) => {
        if (cancelled) return
        upsertTask(task)
        setFetchState('idle')
      })
      .catch((err: unknown) => {
        if (cancelled) return
        const message = err instanceof Error ? err.message : ''
        setFetchState(message.includes('API 404') ? 'missing' : 'failed')
      })
    return () => {
      cancelled = true
    }
  }, [storeTask, taskId, upsertTask])

  useEffect(() => {
    titleRef.current?.focus()
  }, [storeTask?.id])

  if (!storeTask && fetchState === 'loading') {
    return (
      <BeginnerLoadingState
        testId="task-document-loading"
        title="Opening this task"
        detail="Wait a moment while the task loads."
        nextStep="Keep this page open while Forge finds the task."
      />
    )
  }

  if (!storeTask) {
    return (
      <div className="mx-auto max-w-[760px] px-4 py-10 sm:px-6">
        <h1 className="text-ui-doc-title font-medium text-foreground-light dark:text-foreground-dark">
          This task is not on the board anymore.
        </h1>
        <p className="mt-2 text-ui-body text-secondary-light dark:text-secondary-dark">
          {fetchState === 'failed'
            ? 'Check your connection, then open the task board to find current work.'
            : 'Open the task board to find current work, or check the change history.'}
        </p>
        <button
          type="button"
          className={`${uiStyles.primaryButton} mt-4`}
          onClick={() => void navigate({ to: '/tasks' })}
        >
          Open the task board
        </button>
      </div>
    )
  }

  return (
    <div className="flex h-full min-h-0">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[760px] px-4 py-5 sm:px-6">
          <nav
            aria-label="Breadcrumb"
            className="text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            <button
              type="button"
              className="hover:text-foreground-light dark:hover:text-foreground-dark"
              onClick={() => void navigate({ to: '/tasks' })}
            >
              Tasks
            </button>
            <span aria-hidden="true" className="px-1.5">
              ›
            </span>
            <span className="text-foreground-light dark:text-foreground-dark">
              {storeTask.params.task}
            </span>
          </nav>
          <h1
            ref={titleRef}
            tabIndex={-1}
            className="mt-3 text-ui-doc-title font-medium text-foreground-light outline-none dark:text-foreground-dark"
          >
            {storeTask.params.task}
          </h1>
          {/* M3 mounts <TaskDocumentBody task={storeTask} /> here */}
          {/* M5 mounts the activity footer here */}
        </div>
      </div>
      {/* M4 mounts the properties rail aside here */}
    </div>
  )
}
