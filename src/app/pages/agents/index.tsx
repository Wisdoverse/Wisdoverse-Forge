import { useNavigate } from '@tanstack/react-router'
import { useAgentsStore } from '@app/entities/agent'
import { AgentListView } from '@app/features/agents'
import { AgentDetailView } from '@app/widgets/agent-detail'

export function AgentsPage() {
  const { agents, selectedAgentId, selectAgent } = useAgentsStore()
  const navigate = useNavigate()
  const selectedAgent = selectedAgentId
    ? (agents.find((agent) => agent.id === selectedAgentId) ?? null)
    : null

  return (
    <div data-testid="page-agents" className="h-full">
      {selectedAgent ? (
        <div className="h-full overflow-y-auto p-6">
          <AgentDetailView agent={selectedAgent} onBack={() => selectAgent(null)} />
        </div>
      ) : (
        <AgentListView
          onOpenProjectsSetup={() => {
            void navigate({ to: '/settings/$section', params: { section: 'projects' } })
          }}
        />
      )}
    </div>
  )
}
