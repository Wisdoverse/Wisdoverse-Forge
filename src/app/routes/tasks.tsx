import { lazy, Suspense } from 'react'
import { createRoute } from '@tanstack/react-router'
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

function ViewLoadingFallback() {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="animate-pulse text-ui-body text-secondary-light dark:text-secondary-dark">
        Loading…
      </div>
    </div>
  )
}

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/tasks',
  component: function TasksPage() {
    const viewMode = useBoardStore((s) => s.viewMode)

    if (viewMode === 'list')
      return (
        <div data-testid="page-tasks" className="h-full">
          <ListView />
        </div>
      )
    if (viewMode === 'timeline')
      return (
        <div data-testid="page-tasks" className="h-full">
          <Suspense fallback={<ViewLoadingFallback />}>
            <TimelineView />
          </Suspense>
        </div>
      )
    if (viewMode === '3d')
      return (
        <div data-testid="page-tasks" className="h-full">
          <Suspense fallback={<ViewLoadingFallback />}>
            <Workshop3DView />
          </Suspense>
        </div>
      )
    return (
      <div data-testid="page-tasks" className="h-full">
        <BoardView />
      </div>
    )
  },
})
