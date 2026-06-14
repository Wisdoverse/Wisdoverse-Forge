import { lazy, Suspense } from 'react'
import { createRoute, useNavigate } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { BoardView } from '@app/features/board/BoardView'
import { ListView } from '@app/features/list/ListView'
import { useBoardStore } from '@app/shared/model/board.store'

// Lazy-load view bridges to avoid bloating the main bundle
const Workshop3DView = lazy(() =>
  import('@app/widgets/views/Workshop3DView').then((m) => ({ default: m.Workshop3DView }))
)
const TimelineView = lazy(() =>
  import('@app/widgets/views/TimelineView').then((m) => ({ default: m.TimelineView }))
)

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

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/tasks',
  component: function TasksPage() {
    const viewMode = useBoardStore((s) => s.viewMode)
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
  },
})
