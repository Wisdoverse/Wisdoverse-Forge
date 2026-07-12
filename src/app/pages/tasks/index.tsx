import { Suspense } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { BoardView } from '@app/features/board'
import { ListView } from '@app/features/list'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
// Lazy components — the widgets/views barrel owns the lazy() wrappers so both
// stay separate dynamic chunks behind the Suspense boundaries below.
import { TimelineView, Workshop3DView } from '@app/widgets/views'

export function TaskViewLoadingFallback({ viewName }: { viewName: string }) {
  return (
    <div
      data-testid="task-view-loading"
      role="status"
      aria-live="polite"
      className="flex h-full min-h-64 items-center justify-center px-4 text-center"
    >
      <div className="flex max-w-sm flex-col items-center gap-2">
        <div className="h-2 w-2 animate-pulse rounded-full bg-apple-blue" aria-hidden="true" />
        <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          Opening {viewName}
        </p>
        <p className="text-ui-caption leading-relaxed text-secondary-light dark:text-secondary-dark">
          This can take a few seconds the first time. The task board is still available from the
          view switcher.
        </p>
      </div>
    </div>
  )
}

export function TasksPage() {
  const viewMode = useBoardStore((state) => state.viewMode)
  const navigate = useNavigate()

  if (viewMode === 'list')
    return (
      <div data-testid="page-tasks" className="h-full">
        <ListView />
      </div>
    )
  if (viewMode === 'timeline')
    return (
      <div data-testid="page-tasks" className="h-full">
        <Suspense fallback={<TaskViewLoadingFallback viewName="Timeline view" />}>
          <TimelineView />
        </Suspense>
      </div>
    )
  if (viewMode === '3d')
    return (
      <div data-testid="page-tasks" className="h-full">
        <Suspense fallback={<TaskViewLoadingFallback viewName="visual map" />}>
          <Workshop3DView />
        </Suspense>
      </div>
    )
  return (
    <div data-testid="page-tasks" className="h-full">
      <BoardView
        onOpenProjectsSetup={() => {
          void navigate({ to: '/settings/$section', params: { section: 'projects' } })
        }}
        onOpenTaskQueues={() => {
          void navigate({ to: '/agents' })
        }}
      />
    </div>
  )
}
