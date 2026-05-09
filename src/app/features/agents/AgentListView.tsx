import { useEffect, useMemo } from 'react'
import { Bot, Plus } from 'lucide-react'
import { useAgentsStore, type AgentStatus } from '@app/shared/model/agents.store'
import { AgentCard } from './AgentCard'
import { AgentGroupsPanel } from './AgentGroupsPanel'
import { CreateAgentModal } from './CreateAgentModal'

export function AgentListView() {
  const { agents, selectAgent, setCreateModalOpen, loadAgents, loading } = useAgentsStore()
  const statusCounts = useMemo(() => countByStatus(agents), [agents])

  useEffect(() => {
    void loadAgents()
  }, [loadAgents])

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="shrink-0 border-b border-black/5 px-4 py-4 dark:border-white/5 sm:px-6">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4 lg:min-w-[560px]">
            <FleetStat label="Total" value={agents.length} />
            <FleetStat label="Working" value={statusCounts.working} />
            <FleetStat label="Idle" value={statusCounts.idle} />
            <FleetStat label="Offline" value={statusCounts.offline} />
          </div>

          <button
            type="button"
            onClick={() => setCreateModalOpen(true)}
            className="inline-flex h-10 items-center justify-center gap-2 rounded-full bg-apple-blue px-4 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
          >
            <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
            <span>New Agent</span>
          </button>
        </div>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 content-start items-start gap-4 overflow-y-auto px-4 py-5 sm:px-6 xl:grid-cols-[minmax(0,1fr)_320px]">
        <section className="min-w-0">
          <div className="mb-3 flex min-w-0 items-center justify-between gap-3">
            <div className="min-w-0">
              <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                Agent Fleet
              </h2>
              <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Container CLI and provider-backed agents available for tasks.
              </p>
            </div>
            <p className="shrink-0 text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
              {agents.length === 0
                ? 'No agents'
                : `${agents.length} agent${agents.length === 1 ? '' : 's'}`}
            </p>
          </div>

          {loading && agents.length === 0 ? (
            <div className="flex min-h-64 flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-black/10 text-secondary-light dark:border-white/10 dark:text-secondary-dark">
              <p className="text-ui-body">Loading agents…</p>
            </div>
          ) : agents.length === 0 ? (
            <div className="flex min-h-72 flex-col items-center justify-center gap-4 rounded-lg border border-dashed border-black/10 px-6 text-center dark:border-white/10">
              <div className="flex h-14 w-14 items-center justify-center rounded-full bg-apple-blue/10 text-apple-blue">
                <Bot size={28} strokeWidth={1.75} aria-hidden="true" />
              </div>
              <div className="max-w-sm space-y-1">
                <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                  Deploy Your First Agent
                </p>
                <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
                  Agents can run in containers or through a provider prompt. Connect an LLM provider
                  in Settings first, then create the agent here.
                </p>
              </div>
              <button
                type="button"
                onClick={() => setCreateModalOpen(true)}
                className="inline-flex h-10 items-center justify-center gap-2 rounded-full bg-apple-blue px-4 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
              >
                <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
                <span>New Agent</span>
              </button>
            </div>
          ) : (
            <div className="space-y-2">
              {agents.map((agent) => (
                <AgentCard key={agent.id} agent={agent} onClick={() => selectAgent(agent.id)} />
              ))}
            </div>
          )}
        </section>

        <AgentGroupsPanel />
      </div>

      <CreateAgentModal />
    </div>
  )
}

function countByStatus(agents: { status: AgentStatus }[]): Record<AgentStatus, number> {
  return agents.reduce(
    (counts, agent) => {
      counts[agent.status] += 1
      return counts
    },
    { working: 0, idle: 0, offline: 0 } as Record<AgentStatus, number>
  )
}

function FleetStat({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex min-w-0 items-center gap-3 rounded-card border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
      <span className="min-w-0">
        <span className="block text-ui-metric font-semibold tabular-nums text-foreground-light dark:text-foreground-dark">
          {value}
        </span>
        <span className="mt-1 block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {label}
        </span>
      </span>
    </div>
  )
}
