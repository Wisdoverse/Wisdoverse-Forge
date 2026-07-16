import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { TasksPage } from '@app/pages/tasks'
import { TaskDocumentPage } from '@app/pages/task-detail'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/tasks',
  component: TasksPage,
})

export const DetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/tasks/$taskId',
  component: function TaskDetailRoute() {
    const { taskId } = DetailRoute.useParams()
    return <TaskDocumentPage taskId={taskId} />
  },
})
