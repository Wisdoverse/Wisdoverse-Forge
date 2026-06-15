import { useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import {
  ArrowRight,
  ArrowDownUp,
  Bot,
  Check,
  Copy,
  Laptop,
  Monitor,
  Plus,
  Search,
  ShieldCheck,
  Terminal,
} from 'lucide-react'
import {
  isHostCliAgent,
  useAgentsStore,
  type AgentInfo,
  type AgentStatus,
} from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { cn } from '@app/shared/lib/utils'
import { AgentCard } from './AgentCard'
import { AgentGroupsPanel } from './AgentGroupsPanel'
import { CreateAgentModal } from './CreateAgentModal'

type AgentStatusFilter = 'all' | AgentStatus
type AgentRuntimeFilter = 'all' | 'container' | 'host' | 'provider'
type AgentSortKey = 'name' | 'status' | 'active' | 'success'
type HostCliPlatform = 'posix' | 'windows'

interface AgentFilterEmptyCopy {
  title: string
  detail: string
  nextStep: string
}

interface AgentListViewProps {
  onOpenProjectsSetup?: () => void
}

const STATUS_FILTERS: { value: AgentStatusFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'working', label: 'Working now' },
  { value: 'idle', label: 'Ready' },
  { value: 'offline', label: 'Not connected' },
]

const RUNTIME_FILTERS: { value: AgentRuntimeFilter; label: string }[] = [
  { value: 'all', label: 'All agents' },
  { value: 'container', label: 'Managed workspace' },
  { value: 'host', label: 'This computer' },
  { value: 'provider', label: 'Chat-only AI service' },
]

const SORT_OPTIONS: { value: AgentSortKey; label: string }[] = [
  { value: 'name', label: 'Name' },
  { value: 'status', label: 'Status' },
  { value: 'active', label: 'Tasks in progress' },
  { value: 'success', label: 'Best finish rate' },
]

const HOST_CLI_PLATFORMS: {
  value: HostCliPlatform
  label: string
  detail: string
  Icon: typeof Laptop
}[] = [
  {
    value: 'posix',
    label: 'macOS / Linux',
    detail: 'Terminal app',
    Icon: Laptop,
  },
  {
    value: 'windows',
    label: 'Windows',
    detail: 'PowerShell app',
    Icon: Monitor,
  },
]

export function AgentListView({ onOpenProjectsSetup }: AgentListViewProps = {}) {
  const { agents, selectAgent, setCreateModalOpen, loadAgents, loading } = useAgentsStore()
  const selectedProjectId = useNavigationStore((state) => state.selectedProjectId)
  const selectedProjectName = useNavigationStore((state) => {
    if (!state.selectedProjectId) return null
    for (const projects of Object.values(state.projects)) {
      const selectedProject = projects.find((project) => project.id === state.selectedProjectId)
      if (selectedProject) return selectedProject.name
    }
    return null
  })
  const [searchQuery, setSearchQuery] = useState('')
  const [statusFilter, setStatusFilter] = useState<AgentStatusFilter>('all')
  const [runtimeFilter, setRuntimeFilter] = useState<AgentRuntimeFilter>('all')
  const [sortKey, setSortKey] = useState<AgentSortKey>('name')
  const statusCounts = useMemo(() => countByStatus(agents), [agents])
  const runtimeCounts = useMemo(() => countByRuntime(agents), [agents])
  const filteredAgents = useMemo(
    () => filterAndSortAgents(agents, searchQuery, statusFilter, runtimeFilter, sortKey),
    [agents, runtimeFilter, searchQuery, sortKey, statusFilter]
  )
  const agentFilterEmpty = useMemo(
    () => agentFilterEmptyCopy({ searchQuery, statusFilter, runtimeFilter }),
    [runtimeFilter, searchQuery, statusFilter]
  )
  const hasFleetControls = agents.length > 0
  const hasActiveFilter =
    searchQuery.trim().length > 0 || statusFilter !== 'all' || runtimeFilter !== 'all'
  const clearAgentFilters = () => {
    setSearchQuery('')
    setStatusFilter('all')
    setRuntimeFilter('all')
  }

  useEffect(() => {
    void loadAgents()
  }, [loadAgents])

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="grid min-h-0 flex-1 grid-cols-1 content-start items-start gap-4 overflow-y-auto px-4 py-5 sm:px-6 xl:grid-cols-[minmax(0,1fr)_320px]">
        <section className="min-w-0">
          <div className="mb-3 flex min-w-0 items-center justify-between gap-3">
            <div className="min-w-0">
              <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                Agents
              </h2>
              <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Agents that can receive work. Choose one by where the work should happen.
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-3">
              <p className="text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
                {agents.length === 0
                  ? 'No agents'
                  : `${filteredAgents.length}/${agents.length} agent${agents.length === 1 ? '' : 's'}`}
              </p>
              <button
                type="button"
                onClick={() => setCreateModalOpen(true)}
                className="inline-flex h-9 items-center justify-center gap-2 rounded-full bg-apple-blue px-4 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
              >
                <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
                <span>Create Agent</span>
              </button>
            </div>
          </div>

          {hasFleetControls && (
            <>
              <AgentChoiceGuide />
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
            </>
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
                  Create Your First Agent
                </p>
                <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
                  Start with a chat-only AI service for planning and review, or connect this
                  computer when the task needs files and commands on your machine.
                </p>
              </div>
              <button
                type="button"
                onClick={() => setCreateModalOpen(true)}
                className="inline-flex h-10 items-center justify-center gap-2 rounded-full bg-apple-blue px-4 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
              >
                <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
                <span>Create Agent</span>
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
                  {agentFilterEmpty.title}
                </p>
                <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
                  {agentFilterEmpty.detail}
                </p>
                <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
                  {agentFilterEmpty.nextStep}
                </p>
              </div>
              {hasActiveFilter && (
                <button
                  type="button"
                  onClick={clearAgentFilters}
                  className="inline-flex h-9 items-center justify-center rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:border-apple-blue/35 hover:text-apple-blue focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 dark:border-white/[0.1] dark:bg-[#2a2a2c] dark:text-foreground-dark"
                >
                  Show all agents
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
          <AgentGroupsPanel />
          <HostCliEnrollmentPanel
            selectedProjectId={selectedProjectId}
            selectedProjectName={selectedProjectName}
            onOpenProjectsSetup={onOpenProjectsSetup}
          />
        </aside>
      </div>

      <CreateAgentModal />
    </div>
  )
}

function agentFilterEmptyCopy({
  searchQuery,
  statusFilter,
  runtimeFilter,
}: {
  searchQuery: string
  statusFilter: AgentStatusFilter
  runtimeFilter: AgentRuntimeFilter
}): AgentFilterEmptyCopy {
  const hasSearch = searchQuery.trim().length > 0
  const hasStatus = statusFilter !== 'all'
  const hasRuntime = runtimeFilter !== 'all'

  if (hasSearch && !hasStatus && !hasRuntime) {
    return {
      title: 'Search is hiding every agent',
      detail: 'Agents may still exist, but none match the words you typed.',
      nextStep: 'Next: show all agents before creating another one.',
    }
  }

  if (!hasSearch && hasStatus && !hasRuntime) {
    return {
      title: 'This status filter hides every agent',
      detail: 'Agents may still exist in another status, such as working, idle, or offline.',
      nextStep: 'Next: show all agents before deciding nobody is available.',
    }
  }

  if (!hasSearch && !hasStatus && hasRuntime) {
    return {
      title: 'This work location hides every agent',
      detail:
        'Agents may still exist in another place, such as this computer or a managed workspace.',
      nextStep: 'Next: show all agents before deciding one is missing.',
    }
  }

  return {
    title: 'Search and filters are hiding agents',
    detail: 'Agents may still exist, but the current search and filters hide all of them.',
    nextStep: 'Next: show all agents, then narrow the list one choice at a time.',
  }
}

function buildLocalEnrollCommand(
  selectedProjectId: string | null,
  platform: HostCliPlatform
): string {
  const projectArg = selectedProjectId ?? '<project-id>'
  if (platform === 'windows') {
    return [
      'agentforge agents enroll-local `',
      '  --tool codex `',
      '  --name "This Computer Codex" `',
      `  --project ${projectArg} \``,
      '  --cwd "$($PWD.Path)" `',
      '  --shell-format powershell',
    ].join('\n')
  }

  return [
    'agentforge agents enroll-local \\',
    '  --tool codex \\',
    '  --name "This Computer Codex" \\',
    `  --project ${projectArg} \\`,
    '  --cwd "$PWD" \\',
    '  --shell-format bash',
  ].join('\n')
}

function AgentChoiceGuide() {
  return (
    <section
      data-testid="agent-choice-guide"
      className="mb-3 rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
      <div className="flex flex-col gap-1">
        <h3 className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
          Pick by where the work should happen
        </h3>
        <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          Choose the simplest agent that can safely reach the files, tools, or chat needed for the
          task.
        </p>
      </div>
      <div className="mt-3 grid gap-2 md:grid-cols-3">
        <ChoiceGuideItem
          icon={Bot}
          title="Chat-only AI service"
          detail="Best for planning, writing, and review when no project files need to be opened."
        />
        <ChoiceGuideItem
          icon={Laptop}
          title="This computer"
          detail="Best when the task needs the folder, accounts, or tools on your own machine."
        />
        <ChoiceGuideItem
          icon={Terminal}
          title="Managed workspace"
          detail="Best for shared project files that should run inside the Forge workspace."
        />
      </div>
    </section>
  )
}

function ChoiceGuideItem({
  icon: Icon,
  title,
  detail,
}: {
  icon: typeof Bot
  title: string
  detail: string
}) {
  return (
    <div className="flex min-w-0 gap-2 rounded-lg bg-black/[0.025] px-3 py-2 dark:bg-white/[0.04]">
      <Icon
        size={15}
        strokeWidth={2.1}
        className="mt-0.5 shrink-0 text-apple-blue"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </p>
        <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {detail}
        </p>
      </div>
    </div>
  )
}

function HostCliEnrollmentPanel({
  selectedProjectId,
  selectedProjectName,
  onOpenProjectsSetup,
}: {
  selectedProjectId: string | null
  selectedProjectName: string | null
  onOpenProjectsSetup?: () => void
}) {
  const setCreateModalOpen = useAgentsStore((s) => s.setCreateModalOpen)
  const [platform, setPlatform] = useState<HostCliPlatform>('posix')
  const [copied, setCopied] = useState(false)
  const [copyError, setCopyError] = useState<string | null>(null)
  const command = useMemo(
    () => buildLocalEnrollCommand(selectedProjectId, platform),
    [platform, selectedProjectId]
  )
  const projectLabel = selectedProjectId
    ? (selectedProjectName ?? 'Selected project')
    : 'Open project settings first.'
  const commandReady = Boolean(selectedProjectId)

  async function handleCopyCommand() {
    if (!commandReady) return
    setCopyError(null)
    if (!navigator.clipboard?.writeText) {
      setCopyError(
        'Forge cannot copy from this browser. Select the setup command in the box, then copy it manually.'
      )
      return
    }
    try {
      await navigator.clipboard.writeText(command)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1800)
    } catch {
      setCopyError(
        'Forge cannot copy from this browser. Select the setup command in the box, then copy it manually.'
      )
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
              Connect this computer
            </h2>
          </div>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Use this when an agent needs files or commands on your computer. After it connects,
            Forge shows it here and manages it with your other agents.
          </p>
        </div>
        <span className="shrink-0 rounded-full bg-apple-blue/[0.08] px-2 py-1 text-[10px] font-semibold text-apple-blue">
          This computer
        </span>
      </div>

      <button
        type="button"
        onClick={() => setCreateModalOpen(true, 'local-cli')}
        className="mt-3 inline-flex h-9 w-full items-center justify-center gap-2 rounded-full bg-apple-blue px-3 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95"
      >
        <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
        Create agent on this computer
      </button>

      <details className="mt-3">
        <summary className="cursor-pointer text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          If the button does not work
        </summary>
        <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Use this backup if your browser cannot open the setup window or your team asks you to run
          a command. Most people should choose Create agent on this computer above.
        </p>
        <div className="mt-3">
          <p className="mb-2 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
            Computer type
          </p>
          <div
            role="group"
            aria-label="Choose this computer type"
            className="grid grid-cols-2 gap-2"
          >
            {HOST_CLI_PLATFORMS.map((option) => (
              <button
                key={option.value}
                type="button"
                aria-pressed={platform === option.value}
                onClick={() => {
                  setPlatform(option.value)
                  setCopied(false)
                  setCopyError(null)
                }}
                className={cn(
                  'flex min-w-0 items-center gap-2 rounded-lg border px-3 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35',
                  platform === option.value
                    ? 'border-apple-blue/45 bg-apple-blue/[0.08] text-apple-blue'
                    : 'border-black/[0.08] bg-white text-foreground-light hover:border-apple-blue/30 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark'
                )}
              >
                <option.Icon size={15} strokeWidth={2.15} aria-hidden="true" />
                <span className="min-w-0">
                  <span className="block truncate text-ui-button font-medium">{option.label}</span>
                  <span className="block truncate text-[10px] text-secondary-light dark:text-secondary-dark">
                    {option.detail}
                  </span>
                </span>
              </button>
            ))}
          </div>
        </div>

        <div className="mt-3 flex items-center gap-2 rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.04]">
          <ShieldCheck
            size={15}
            strokeWidth={2.1}
            className="shrink-0 text-apple-green"
            aria-hidden="true"
          />
          <p
            data-testid="host-cli-project-label"
            className="min-w-0 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Project:{' '}
            <span className="font-medium text-foreground-light dark:text-foreground-dark">
              {projectLabel}
            </span>
          </p>
        </div>

        {commandReady ? (
          <>
            <pre className="mt-3 max-h-36 overflow-auto rounded-lg bg-[#111318] p-3 text-left font-mono text-[11px] leading-relaxed text-white/85">
              <code className="whitespace-pre-wrap break-all">{command}</code>
            </pre>

            <div className="mt-3 grid gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
              <p>
                1. Open Terminal on macOS/Linux or PowerShell on Windows in the folder this agent
                should work in.
              </p>
              <p>
                2. Copy this setup command and paste it into that Terminal or PowerShell window.
              </p>
              <p>3. Leave the work tool as Codex unless your team tells you otherwise.</p>
            </div>
            <p
              data-testid="host-cli-success-hint"
              className="mt-3 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              When it works, come back to Forge. A new agent named This Computer Codex appears in
              this list. Keep the command window open while it works.
            </p>
          </>
        ) : (
          <div
            data-testid="host-cli-command-waiting"
            className="mt-3 rounded-lg border border-dashed border-black/[0.12] px-3 py-3 text-ui-caption text-secondary-light dark:border-white/[0.12] dark:text-secondary-dark"
          >
            <p>
              Open project settings to create a project, or choose an existing project from the
              project list. Then the setup command appears here.
            </p>
            {onOpenProjectsSetup ? (
              <button
                type="button"
                onClick={onOpenProjectsSetup}
                className="mt-3 inline-flex h-8 items-center justify-center gap-1.5 rounded-full border border-apple-blue/20 bg-apple-blue/[0.08] px-3 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/[0.12] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35"
              >
                <span>Open project settings</span>
                <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
              </button>
            ) : null}
          </div>
        )}

        <button
          type="button"
          onClick={() => void handleCopyCommand()}
          disabled={!commandReady}
          className={cn(
            'mt-4 inline-flex h-9 w-full items-center justify-center gap-2 rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark',
            commandReady
              ? 'hover:border-apple-blue/35 hover:text-apple-blue'
              : 'cursor-not-allowed opacity-60'
          )}
        >
          {copied ? (
            <Check size={14} strokeWidth={2.25} aria-hidden="true" />
          ) : (
            <Copy size={14} strokeWidth={2.25} aria-hidden="true" />
          )}
          <span>
            {commandReady ? (copied ? 'Copied' : 'Copy setup command') : 'Choose project first'}
          </span>
        </button>
        {copyError && (
          <p role="alert" className="mt-2 text-ui-caption font-medium text-apple-red">
            {copyError}
          </p>
        )}
      </details>
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
            placeholder="Search agents, AI services, projects…"
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

        <FilterButtonGroup label="Work location">
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
