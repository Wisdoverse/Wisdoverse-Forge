import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { TasksPage } from '@app/pages/tasks'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/tasks',
  component: TasksPage,
})
