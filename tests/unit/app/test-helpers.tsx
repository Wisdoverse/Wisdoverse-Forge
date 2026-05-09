import { createRouter, createRootRoute, createRoute, redirect } from '@tanstack/react-router'
import type { MemoryHistory } from '@tanstack/react-router'
import { Outlet } from '@tanstack/react-router'

function placeholder(name: string) {
  return function Page() {
    return <div data-testid={`page-${name}`}>{name}</div>
  }
}

export function createTestRouter(history: MemoryHistory) {
  const rootRoute = createRootRoute({
    component: () => <Outlet />,
  })

  const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/',
    beforeLoad: () => {
      throw redirect({ to: '/tasks' })
    },
  })

  const tasksRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/tasks',
    component: placeholder('tasks'),
  })
  const inboxRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/inbox',
    component: placeholder('inbox'),
  })
  const agentsRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/agents',
    component: placeholder('agents'),
  })
  const skillsRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/skills',
    component: placeholder('skills'),
  })
  const settingsRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/settings',
    component: placeholder('settings'),
  })
  const settingsSectionRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/settings/$section',
    component: placeholder('settings'),
  })

  const routeTree = rootRoute.addChildren([
    indexRoute,
    tasksRoute,
    inboxRoute,
    agentsRoute,
    skillsRoute,
    settingsRoute,
    settingsSectionRoute,
  ])
  return createRouter({ routeTree, history })
}
