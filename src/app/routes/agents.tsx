import { createRoute } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { useAgentsStore } from '@app/shared/model/agents.store'
import { AgentListView } from '@app/features/agents/AgentListView'
import { AgentDetailView } from '@app/widgets/agent-detail/AgentDetailView'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/agents',
  component: function AgentsPage() {
    const { agents, selectedAgentId, selectAgent } = useAgentsStore()
    const selectedAgent = selectedAgentId
      ? (agents.find((a) => a.id === selectedAgentId) ?? null)
      : null

    return (
      <div data-testid="page-agents" className="h-full">
        {selectedAgent ? (
          <div className="h-full overflow-y-auto p-6">
            <AgentDetailView agent={selectedAgent} onBack={() => selectAgent(null)} />
          </div>
        ) : (
          <AgentListView />
        )}
      </div>
    )
  },
})
