import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { InboxView } from '@app/features/inbox/InboxView'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/inbox',
  component: function InboxPage() {
    return (
      <div data-testid="page-inbox" className="h-full">
        <InboxView />
      </div>
    )
  },
})
