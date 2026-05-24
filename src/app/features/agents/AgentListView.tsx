import { useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { ArrowDownUp, Bot, Check, Copy, Plus, Search, ShieldCheck, Terminal } from 'lucide-react'
import {
  isHostCliAgent,
  useAgentsStore,
  type AgentInfo,
  type AgentStatus,
} from '@app/shared/model/agents.store'
import { useNavigationStore } from '@app/entities/navigation'
import { cn } from '@app/shared/lib/utils'
import { AgentCard } from './AgentCard'
import { AgentGroupsPanel } from './AgentGroupsPanel'
import { CreateAgentModal } from './CreateAgentModal'

type AgentStatusFilter = 'all' | AgentStatus
type AgentRuntimeFilter = 'all' | 'container' | 'host' | 'provider'
type AgentSortKey = 'name' | 'status' | 'active' | 'success'

const STATUS_FILTERS: { value: AgentStatusFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'working', label: 'Working' },
  { value: 'idle', label: 'Idle' },
  { value: 'offline', label: 'Offline' },
]

const RUNTIME_FILTERS: { value: AgentRuntimeFilter; label: string }[] = [
  { value: 'all', label: 'All Runtimes' },
  { value: 'container', label: 'Container CLI' },
  { value: 'host', label: 'Host CLI' },
  { value: 'provider', label: 'Provider' },
]

const SORT_OPTIONS: { value: AgentSortKey; label: string }[] = [
  { value: 'name', label: 'Name' },
  { value: 'status', label: 'Status' },
  { value: 'active', label: 'Active Work' },
  { value: 'success', label: 'Success Rate' },
]

export function AgentListView() {
  const { agents, selectAgent, setCreateModalOpen, loadAgents, loading } = useAgentsStore()
  const selectedProjectId = useNavigationStore((state) => state.selectedProjectId)
  const [searchQuery, setSearchQuery] = useState('')
  const [statusFilter, setStatusFilter] = useState<AgentStatusFilter>('all')
  const [runtimeFilter, setRuntimeFilter] = useState<AgentRuntimeFilter>('all')
  const [sortKey, setSortKey] = useState<AgentSortKey>('name')
  const localEnrollCommand = useMemo(
    () => buildLocalEnrollCommand(selectedProjectId),
    [selectedProjectId]
  )
  const statusCounts = useMemo(() => countByStatus(agents), [agents])
  const runtimeCounts = useMemo(() => countByRuntime(agents), [agents])
  const filteredAgents = useMemo(
    () => filterAndSortAgents(agents, searchQuery, statusFilter, runtimeFilter, sortKey),
    [agents, runtimeFilter, searchQuery, sortKey, statusFilter]
  )
  const hasFleetControls = agents.length > 0
  const hasActiveFilter =
    searchQuery.trim().length > 0 || statusFilter !== 'all' || runtimeFilter !== 'all'

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
                Container, Host CLI, and provider-backed agents available for tasks.
              </p>
            </div>
            <p className="shrink-0 text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
              {agents.length === 0
                ? 'No agents'
                : `${filteredAgents.length}/${agents.length} agent${agents.length === 1 ? '' : 's'}`}
            </p>
          </div>

          {hasFleetControls && (
            <FleetControls
              searchQuery={searchQuery}
              onSearchQueryChange={setSearchQuery}
              statusFilter={statusFilter}
              onStatusFilterChange={setStatusFilter}
              statusCounts={statusCounts}
              runtimeFilter={runtimeFilter}
              onRuntimeFilterChange={setRuntimeFilter}
              runtimeCounts={runtimeCounts}
              sortKey={sortKey}
              onSortKeyChange={setSortKey}
            />
          )}

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
          ) : filteredAgents.length === 0 ? (
            <div
              data-testid="agent-filter-empty"
              className="flex min-h-64 flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-black/10 px-6 text-center dark:border-white/10"
            >
              <Search
                size={28}
                strokeWidth={1.75}
                className="text-secondary-light dark:text-secondary-dark"
                aria-hidden="true"
              />
              <div className="max-w-sm space-y-1">
                <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                  No Agents Match This View
                </p>
                <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
                  Clear search or switch filters to review every agent.
                </p>
              </div>
              {hasActiveFilter && (
                <button
                  type="button"
                  onClick={() => {
                    setSearchQuery('')
                    setStatusFilter('all')
                    setRuntimeFilter('all')
                  }}
                  className="inline-flex h-9 items-center justify-center rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:border-apple-blue/35 hover:text-apple-blue focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 dark:border-white/[0.1] dark:bg-[#2a2a2c] dark:text-foreground-dark"
                >
                  Clear Filters
                </button>
              )}
            </div>
          ) : (
            <div className="space-y-2">
              {filteredAgents.map((agent) => (
                <AgentCard key={agent.id} agent={agent} onClick={() => selectAgent(agent.id)} />
              ))}
            </div>
          )}
        </section>

        <aside className="space-y-4 xl:sticky xl:top-0 xl:self-start">
          <HostCliEnrollmentPanel
            command={localEnrollCommand}
            selectedProjectId={selectedProjectId}
          />
          <AgentGroupsPanel />
        </aside>
      </div>

      <CreateAgentModal />
    </div>
  )
}

function buildLocalEnrollCommand(selectedProjectId: string | null): string {
  const projectArg = selectedProjectId ?? '<project-id>'
  return [
    'agentforge agents enroll-local \\',
    '  --tool codex \\',
    '  --name "Host Codex" \\',
    `  --project ${projectArg} \\`,
    '  --cwd "$PWD"',
  ].join('\n')
}

function HostCliEnrollmentPanel({
  command,
  selectedProjectId,
}: {
  command: string
  selectedProjectId: string | null
}) {
  const [copied, setCopied] = useState(false)
  const projectLabel = selectedProjectId ?? 'Select a project'

  async function handleCopyCommand() {
    if (!navigator.clipboard?.writeText) return
    try {
      await navigator.clipboard.writeText(command)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1800)
    } catch {
      // Copy is a convenience action; the command remains visible when unsupported.
    }
  }

  return (
    <section
      data-testid="host-cli-enrollment-panel"
      className="rounded-card border border-black/[0.08] bg-white p-5 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Terminal
              size={15}
              strokeWidth={2}
              className="text-secondary-light dark:text-secondary-dark"
              aria-hidden="true"
            />
            <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              Connect Host CLI
            </h2>
          </div>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Add a local machine as a managed agent for this project.
          </p>
        </div>
        <span className="shrink-0 rounded-full bg-apple-blue/[0.08] px-2 py-1 text-[10px] font-semibold text-apple-blue">
          Managed
        </span>
      </div>

      <div className="mt-3 flex items-center gap-2 rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.04]">
        <ShieldCheck size={15} strokeWidth={2.1} className="shrink-0 text-apple-green" />
        <p className="min-w-0 truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          Project: <span className="font-mono">{projectLabel}</span>
        </p>
      </div>

      <pre className="mt-3 max-h-36 overflow-auto rounded-lg bg-[#111318] p-3 text-left font-mono text-[11px] leading-relaxed text-white/85">
        <code className="whitespace-pre-wrap break-all">{command}</code>
      </pre>

      <div className="mt-3 grid gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
        <p>Platform CLI enrolls the agent identity.</p>
        <p>The local sidecar sends heartbeats, task results, and evidence.</p>
        <p>Container actions stay hidden for Host CLI agents.</p>
      </div>

      <button
        type="button"
        onClick={() => void handleCopyCommand()}
        className="mt-4 inline-flex h-9 w-full items-center justify-center gap-2 rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:border-apple-blue/35 hover:text-apple-blue focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
      >
        {copied ? (
          <Check size={14} strokeWidth={2.25} aria-hidden="true" />
        ) : (
          <Copy size={14} strokeWidth={2.25} aria-hidden="true" />
        )}
        <span>{copied ? 'Copied' : 'Copy command'}</span>
      </button>
    </section>
  )
}

function filterAndSortAgents(
  agents: AgentInfo[],
  searchQuery: string,
  statusFilter: AgentStatusFilter,
  runtimeFilter: AgentRuntimeFilter,
  sortKey: AgentSortKey
): AgentInfo[] {
  const query = searchQuery.trim().toLowerCase()
  const filtered = agents.filter((agent) => {
    if (statusFilter !== 'all' && agent.status !== statusFilter) return false
    if (runtimeFilter === 'container' && (!agent.cliTool || isHostCliAgent(agent))) return false
    if (runtimeFilter === 'host' && !isHostCliAgent(agent)) return false
    if (runtimeFilter === 'provider' && agent.cliTool) return false
    if (!query) return true
    return agentSearchText(agent).includes(query)
  })

  return filtered.sort((a, b) => {
    switch (sortKey) {
      case 'status':
        return statusRank(a.status) - statusRank(b.status) || a.name.localeCompare(b.name)
      case 'active':
        return b.tasksInProgress - a.tasksInProgress || a.name.localeCompare(b.name)
      case 'success':
        return b.successRate - a.successRate || a.name.localeCompare(b.name)
      case 'name':
      default:
        return a.name.localeCompare(b.name)
    }
  })
}

function agentSearchText(agent: AgentInfo): string {
  return [
    agent.name,
    agent.provider,
    agent.model,
    agent.cliTool,
    agent.cwd,
    agent.workspaceName,
    agent.projectName,
    agent.currentTask,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
}

function statusRank(status: AgentStatus): number {
  switch (status) {
    case 'working':
      return 0
    case 'idle':
      return 1
    case 'offline':
      return 2
  }
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

function countByRuntime(agents: AgentInfo[]): Record<AgentRuntimeFilter, number> {
  return agents.reduce(
    (counts, agent) => {
      counts.all += 1
      if (isHostCliAgent(agent)) counts.host += 1
      else if (agent.cliTool) counts.container += 1
      else counts.provider += 1
      return counts
    },
    { all: 0, container: 0, host: 0, provider: 0 } as Record<AgentRuntimeFilter, number>
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

interface FleetControlsProps {
  searchQuery: string
  onSearchQueryChange: (query: string) => void
  statusFilter: AgentStatusFilter
  onStatusFilterChange: (filter: AgentStatusFilter) => void
  statusCounts: Record<AgentStatus, number>
  runtimeFilter: AgentRuntimeFilter
  onRuntimeFilterChange: (filter: AgentRuntimeFilter) => void
  runtimeCounts: Record<AgentRuntimeFilter, number>
  sortKey: AgentSortKey
  onSortKeyChange: (sortKey: AgentSortKey) => void
}

function FleetControls({
  searchQuery,
  onSearchQueryChange,
  statusFilter,
  onStatusFilterChange,
  statusCounts,
  runtimeFilter,
  onRuntimeFilterChange,
  runtimeCounts,
  sortKey,
  onSortKeyChange,
}: FleetControlsProps) {
  return (
    <div
      data-testid="agent-fleet-controls"
      className="mb-3 grid gap-3 rounded-lg border border-black/[0.08] bg-white p-3 dark:border-white/[0.1] dark:bg-[#2a2a2c] xl:grid-cols-[minmax(220px,0.7fr)_minmax(0,1.3fr)_auto]"
    >
      <div className="min-w-0">
        <label htmlFor="agent-search" className="sr-only">
          Search Agents
        </label>
        <div className="relative">
          <Search
            size={15}
            strokeWidth={2}
            className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
            aria-hidden="true"
          />
          <input
            id="agent-search"
            data-testid="agent-search"
            name="agent-search"
            type="search"
            autoComplete="off"
            value={searchQuery}
            onChange={(event) => onSearchQueryChange(event.target.value)}
            placeholder="Search agents, models, projects…"
            className="h-10 w-full rounded-md border border-black/[0.08] bg-white pl-9 pr-3 text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light focus-visible:border-apple-blue/40 focus-visible:ring-2 focus-visible:ring-apple-blue/20 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark"
          />
        </div>
      </div>

      <div className="flex min-w-0 flex-col gap-2">
        <FilterButtonGroup label="Status">
          {STATUS_FILTERS.map((filter) => (
            <FilterButton
              key={filter.value}
              active={statusFilter === filter.value}
              label={filter.label}
              count={filter.value === 'all' ? runtimeCounts.all : statusCounts[filter.value]}
              onClick={() => onStatusFilterChange(filter.value)}
            />
          ))}
        </FilterButtonGroup>

        <FilterButtonGroup label="Runtime">
          {RUNTIME_FILTERS.map((filter) => (
            <FilterButton
              key={filter.value}
              active={runtimeFilter === filter.value}
              label={filter.label}
              count={runtimeCounts[filter.value]}
              onClick={() => onRuntimeFilterChange(filter.value)}
            />
          ))}
        </FilterButtonGroup>
      </div>

      <div className="flex items-center gap-2 xl:justify-end">
        <ArrowDownUp
          size={15}
          strokeWidth={2}
          className="text-secondary-light dark:text-secondary-dark"
          aria-hidden="true"
        />
        <label htmlFor="agent-sort" className="sr-only">
          Sort Agents
        </label>
        <select
          id="agent-sort"
          data-testid="agent-sort"
          name="agent-sort"
          value={sortKey}
          onChange={(event) => onSortKeyChange(event.target.value as AgentSortKey)}
          className="h-10 rounded-md border border-black/[0.08] bg-white px-3 text-ui-body text-foreground-light outline-none transition-colors focus-visible:border-apple-blue/40 focus-visible:ring-2 focus-visible:ring-apple-blue/20 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
        >
          {SORT_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              Sort: {option.label}
            </option>
          ))}
        </select>
      </div>
    </div>
  )
}

function FilterButtonGroup({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div
      className="flex min-w-0 flex-wrap items-center gap-1.5"
      role="group"
      aria-label={`${label} Filter`}
    >
      <span className="mr-1 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
        {label}
      </span>
      {children}
    </div>
  )
}

function FilterButton({
  active,
  label,
  count,
  onClick,
}: {
  active: boolean
  label: string
  count: number
  onClick: () => void
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        'inline-flex h-7 min-w-0 items-center gap-1.5 rounded-full border px-2.5 text-ui-caption font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35',
        active
          ? 'border-apple-blue/45 bg-apple-blue/[0.08] text-apple-blue'
          : 'border-black/[0.08] bg-white text-secondary-light hover:border-apple-blue/30 hover:text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark dark:hover:text-foreground-dark'
      )}
    >
      <span className="truncate">{label}</span>
      <span className="tabular-nums opacity-70">{count}</span>
    </button>
  )
}
