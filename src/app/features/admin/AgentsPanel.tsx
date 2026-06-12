import { useEffect } from 'react'
import { Bot, Cpu, Server, Sparkles, type LucideIcon } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import {
  type AdminAgent,
  type AdminAgentRuntimeKindFilter,
  useAdminStore,
} from '@app/shared/model/admin.store'
import {
  type AgentRuntimeKind,
  RUNTIME_KINDS,
  runtimeKindLabel,
  runtimeKindShortLabel,
} from '@app/entities/agent'
import { ADMIN_PANEL_RECOVERY, adminPanelLoadErrorMessage } from './adminErrorCopy'

// ============================================================================
// Filter options
// ============================================================================

interface FilterOption {
  value: AdminAgentRuntimeKindFilter
  label: string
}

const RUNTIME_KIND_FILTER_OPTIONS: FilterOption[] = [
  { value: 'all', label: 'All work locations' },
  ...RUNTIME_KINDS.map((kind) => ({ value: kind, label: runtimeKindLabel(kind) })),
]

// ============================================================================
// Helpers
// ============================================================================

function formatLastActivity(epochMs: number): string {
  if (!epochMs) return '—'
  try {
    return new Date(epochMs).toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    })
  } catch {
    return '—'
  }
}

function agentStatusLabel(status: string): string {
  switch (status.trim().toLowerCase()) {
    case 'idle':
      return 'Ready'
    case 'working':
      return 'Working'
    case 'offline':
      return 'Offline'
    default:
      return status.trim() ? 'Needs review' : 'Status not reported'
  }
}

const RUNTIME_KIND_BADGE_STYLES: Record<AgentRuntimeKind, string> = {
  container: 'bg-apple-blue/10 text-apple-blue',
  cli: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
  api: 'bg-apple-blue/[0.07] text-apple-blue',
}

function AgentKindBadge({ kind }: { kind: AgentRuntimeKind }) {
  return (
    <span
      data-testid={`agent-kind-badge-${kind}`}
      title={runtimeKindLabel(kind)}
      className={cn(
        'inline-flex items-center rounded-full px-2 py-0.5 text-ui-caption font-medium',
        RUNTIME_KIND_BADGE_STYLES[kind] ?? uiStyles.badge
      )}
    >
      {runtimeKindShortLabel(kind)}
    </span>
  )
}

// ============================================================================
// Guide
// ============================================================================

const AGENT_GUIDANCE: { title: string; description: string; Icon: LucideIcon }[] = [
  {
    title: 'Managed workspace',
    description: 'Runs file and command work in a platform-managed workspace. Best for most teams.',
    Icon: Server,
  },
  {
    title: 'This computer',
    description:
      'Runs work from a joined computer. Use it when files or tools must stay on that machine.',
    Icon: Cpu,
  },
  {
    title: 'Chat-only AI service',
    description:
      'Uses a connected AI service for planning and review. It does not open files or run commands.',
    Icon: Sparkles,
  },
]

function agentsSummary(agents: AdminAgent[], filter: AdminAgentRuntimeKindFilter): string {
  if (agents.length === 0) {
    return filter === 'all'
      ? 'No agents have been created across any team space yet.'
      : `No ${runtimeKindLabel(filter)} agents are present right now.`
  }
  const scope = filter === 'all' ? 'all work locations' : runtimeKindLabel(filter)
  return `Showing ${agents.length} agent${agents.length === 1 ? '' : 's'} (${scope}).`
}

function AgentsGuide({
  agents,
  filter,
}: {
  agents: AdminAgent[]
  filter: AdminAgentRuntimeKindFilter
}) {
  return (
    <section
      data-testid="admin-agents-guide"
      className="mb-4 rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2c2c2e]"
    >
      <div className="mb-3">
        <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          Admin view
        </p>
        <h3 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          Filter agents by how they work
        </h3>
        <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
          {agentsSummary(agents, filter)}
        </p>
      </div>
      <div className="grid gap-2 md:grid-cols-3">
        {AGENT_GUIDANCE.map(({ title, description, Icon }) => (
          <div key={title} className="rounded-lg bg-black/[0.03] p-3 dark:bg-white/[0.04]">
            <div className="mb-2 flex items-center gap-2 text-foreground-light dark:text-foreground-dark">
              <Icon size={14} strokeWidth={2.2} className="shrink-0 text-apple-blue" />
              <p className="text-ui-caption font-semibold">{title}</p>
            </div>
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {description}
            </p>
          </div>
        ))}
      </div>
    </section>
  )
}

function AgentsEmptyState({ filter }: { filter: AdminAgentRuntimeKindFilter }) {
  return (
    <div
      data-testid="admin-agents-empty"
      className="flex flex-col items-center justify-center px-4 py-12 text-center"
    >
      <div
        className="mb-3 flex h-10 w-10 items-center justify-center rounded-lg bg-black/[0.03] text-secondary-light dark:bg-white/[0.05] dark:text-secondary-dark"
        aria-hidden="true"
      >
        <Bot size={18} strokeWidth={2} />
      </div>
      <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
        No agents to show
      </p>
      <p className="mt-1 max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
        {filter === 'all'
          ? 'Create the first agent from Agents, confirm it becomes Ready or Working, then return here to review it across team spaces. If you just created one, refresh Admin and check again.'
          : `No ${runtimeKindLabel(filter)} agents match this filter. Choose "All work locations" before assuming the agent is missing.`}
      </p>
    </div>
  )
}

// ============================================================================
// AgentsPanel
// ============================================================================

export function AgentsPanel() {
  const {
    agents,
    agentsLoading,
    agentsError,
    agentRuntimeKindFilter,
    loadAgents,
    setAgentRuntimeKindFilter,
  } = useAdminStore()

  useEffect(() => {
    void loadAgents()
  }, [loadAgents])

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Agents</h2>
          <p className={uiStyles.sectionDescription}>
            Review agents across every team space and filter them by work location.
          </p>
        </div>
        <div>
          <label htmlFor="admin-agents-runtime-filter" className={uiStyles.label}>
            Work location
          </label>
          <select
            id="admin-agents-runtime-filter"
            data-testid="admin-agents-runtime-filter"
            aria-label="Filter agents by work location"
            value={agentRuntimeKindFilter}
            onChange={(event) =>
              void setAgentRuntimeKindFilter(event.target.value as AdminAgentRuntimeKindFilter)
            }
            className={uiStyles.select}
          >
            {RUNTIME_KIND_FILTER_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      {agentsError && (
        <div data-testid="admin-agents-error" role="alert" className={uiStyles.error}>
          <p>{adminPanelLoadErrorMessage(agentsError, 'agents')}</p>
          <p className="mt-1 text-ui-caption">{ADMIN_PANEL_RECOVERY}</p>
        </div>
      )}

      <AgentsGuide agents={agents} filter={agentRuntimeKindFilter} />

      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {agentsLoading && agents.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Loading agents…
            </p>
          </div>
        ) : agents.length === 0 ? (
          <AgentsEmptyState filter={agentRuntimeKindFilter} />
        ) : (
          <table className={uiStyles.table}>
            <thead className={uiStyles.tableHead}>
              <tr>
                <th className={uiStyles.tableHeaderCell}>Name</th>
                <th className={uiStyles.tableHeaderCell}>Work location</th>
                <th className={uiStyles.tableHeaderCell}>Status</th>
                <th className={uiStyles.tableHeaderCell}>Owner</th>
                <th className={uiStyles.tableHeaderCell}>Project</th>
                <th className={uiStyles.tableHeaderCell}>Last activity</th>
              </tr>
            </thead>
            <tbody>
              {agents.map((agent) => (
                <tr
                  key={agent.id}
                  data-testid="admin-agent-row"
                  className="border-b border-black/[0.06] transition-colors hover:bg-black/[0.02] dark:border-white/[0.08] dark:hover:bg-white/[0.02]"
                >
                  <td className={uiStyles.tableCell}>
                    <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                      {agent.name || 'Unnamed agent'}
                    </p>
                    {agent.cliTool && (
                      <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                        Work tool: {agent.cliTool}
                      </p>
                    )}
                  </td>
                  <td className={uiStyles.tableCell}>
                    <AgentKindBadge kind={agent.runtimeKind} />
                  </td>
                  <td
                    className={cn(
                      uiStyles.tableCell,
                      'text-ui-body text-foreground-light dark:text-foreground-dark'
                    )}
                  >
                    {agentStatusLabel(agent.status)}
                  </td>
                  <td
                    className={cn(
                      uiStyles.tableCell,
                      'text-ui-caption text-secondary-light dark:text-secondary-dark'
                    )}
                  >
                    {agent.ownerUsername ?? agent.ownerEmail ?? '—'}
                  </td>
                  <td
                    className={cn(
                      uiStyles.tableCell,
                      'text-ui-caption text-secondary-light dark:text-secondary-dark'
                    )}
                  >
                    {agent.projectName ?? '—'}
                  </td>
                  <td
                    className={cn(
                      uiStyles.tableCell,
                      'text-ui-caption text-secondary-light dark:text-secondary-dark'
                    )}
                  >
                    {formatLastActivity(agent.lastActivity)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
