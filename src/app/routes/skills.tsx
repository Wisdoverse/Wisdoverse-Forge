import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { SkillsPage } from '@app/pages/skills'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/skills',
  component: SkillsPage,
})
