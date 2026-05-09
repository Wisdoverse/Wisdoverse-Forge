import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { SkillsView } from '@app/features/skills/SkillsView'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/skills',
  component: function SkillsPage() {
    return (
      <div data-testid="page-skills" className="h-full">
        <SkillsView />
      </div>
    )
  },
})
