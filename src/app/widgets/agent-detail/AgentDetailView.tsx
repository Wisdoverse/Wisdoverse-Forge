import { lazy, Suspense, useEffect, useState } from 'react'
import { Info } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import {
  isHostCliAgent,
  useAgentsStore,
  type AgentInfo,
  type AgentStatus,
} from '@app/shared/model/agents.store'
import { AgentConfigTab } from '@app/features/agents/AgentConfigTab'
import { AgentControlPanel } from '@app/features/agents/AgentControlPanel'
import { AgentKindBadge } from '@app/features/agents/AgentKindBadge'
import { AgentPluginsTab } from '@app/features/agents/AgentPluginsTab'
import { AgentTasksTab } from '@app/features/agents/AgentTasksTab'
import { ChatView } from '@app/features/chat/ChatView'
import { orchestrationApi, type TaskSummary } from '@app/shared/api/orchestration'
import { formatRelativeTime } from '@app/shared/lib/time'

// Lazy — keeps xterm out of the agents route's initial chunk
const AgentTerminalTab = lazy(() =>
  import('@app/features/agents/AgentTerminalTab').then((m) => ({ default: m.AgentTerminalTab }))
)

const PROVIDER_GRADIENTS: Record<string, string> = {
  Anthropic: 'bg-[#f5f5f7] text-[#1d1d1f] dark:bg-white/[0.08] dark:text-white',
  Google: 'bg-[#f5f5f7] text-[#1d1d1f] dark:bg-white/[0.08] dark:text-white',
  OpenAI: 'bg-[#f5f5f7] text-[#1d1d1f] dark:bg-white/[0.08] dark:text-white',
}

const STATUS_COLORS: Record<AgentStatus, string> = {
  working: 'bg-[#1d1d1f] dark:bg-white',
  idle: 'bg-[#7a7a7a]',
  offline: 'bg-[#d2d2d7]',
}

const STATUS_LABELS: Record<AgentStatus, string> = {
  working: 'Working',
  idle: 'Idle',
  offline: 'Offline',
}

function defaultGradient(provider: string): string {
  return (
    PROVIDER_GRADIENTS[provider] ??
    'bg-[#f5f5f7] text-[#1d1d1f] dark:bg-white/[0.08] dark:text-white'
  )
}

type Tab = 'overview' | 'tasks' | 'history' | 'terminal' | 'plugins' | 'config'

// Terminal attach is only available for platform-managed containers. Host CLI
// agents are still task-managed through sidecar/NATS, but their local terminal
// stays on the enrolled machine.
function tabsFor(agent: AgentInfo): { id: Tab; label: string }[] {
  const isCli = Boolean(agent.cliTool)
  const hasTerminal = isCli && !isHostCliAgent(agent)
  return [
    { id: 'overview', label: 'Overview' },
    { id: 'tasks', label: 'Tasks' },
    { id: 'history', label: isCli ? 'History' : 'Chat' },
    ...(hasTerminal ? [{ id: 'terminal' as Tab, label: 'Terminal' }] : []),
    { id: 'plugins', label: 'Plugins' },
    { id: 'config', label: 'Config' },
  ]
}

function WorkspaceBoundaryNote({ agent }: { agent: AgentInfo }) {
  const hostCli = isHostCliAgent(agent)
  return (
    <div className="flex gap-2 rounded-lg bg-apple-blue/10 px-3 py-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
      <Info
        size={13}
        strokeWidth={2.25}
        className="mt-0.5 shrink-0 text-apple-blue"
        aria-hidden="true"
      />
      {hostCli ? (
        <p>
          Host CLI agents run on the enrolled machine. The platform manages identity, task
          assignment, heartbeats, and result evidence; filesystem access remains the local directory
          where the sidecar runs.
        </p>
      ) : agent.cliTool ? (
        <p>
          /workspace is mounted from the shared workspace and may include multiple projects. Primary
          Project only sets default task context. Only Container CLI agents use this mount; Provider
          + Prompt agents do not access files directly. Use a separate workspace for strict
          filesystem isolation.
        </p>
      ) : (
        <p>
          Provider + Prompt agents do not mount /workspace or read files directly. Primary Project
          only sets default task context. Use a Container CLI agent when filesystem tools are
          required.
        </p>
      )}
    </div>
  )
}

interface AgentDetailViewProps {
  agent: AgentInfo
  onBack: () => void
}

export function AgentDetailView({ agent, onBack }: AgentDetailViewProps) {
  const [activeTab, setActiveTab] = useState<Tab>('overview')
  const [recentTasks, setRecentTasks] = useState<TaskSummary[]>([])
  const ratePercent = Math.round(agent.successRate * 100)
  const tabs = tabsFor(agent)

  useEffect(() => {
    if (!tabs.some((tab) => tab.id === activeTab)) setActiveTab('overview')
  }, [activeTab, tabs])

  useEffect(() => {
    let cancelled = false
    orchestrationApi
      .getTasksByAgent(agent.id, { limit: 5 })
      .then((tasks) => {
        if (!cancelled) setRecentTasks(tasks)
      })
      .catch(() => {
        if (!cancelled) setRecentTasks([])
      })
    return () => {
      cancelled = true
    }
  }, [agent.id])

  return (
    <div className="flex flex-col gap-4">
      {/* Header */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          data-testid="agent-back"
          onClick={onBack}
          className={cn(
            'flex items-center justify-center w-8 h-8 rounded-lg shrink-0',
            'bg-white dark:bg-[#2a2a2c] border border-black/[0.08] dark:border-white/[0.1]',
            'text-secondary-light dark:text-secondary-dark hover:text-foreground-light dark:hover:text-foreground-dark',
            'transition-colors'
          )}
          aria-label="Back"
        >
          ‹
        </button>

        {/* Provider-colored avatar */}
        <div
          className={cn(
            'w-10 h-10 rounded-xl flex items-center justify-center shrink-0',
            'border border-black/[0.08] text-ui-body font-semibold select-none dark:border-white/[0.1]',
            defaultGradient(agent.provider)
          )}
        >
          {agent.provider.charAt(0).toUpperCase()}
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h1 className="truncate text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
              {agent.name}
            </h1>
            <AgentKindBadge cliTool={agent.cliTool} runtimeKind={agent.runtimeKind} />
          </div>
          <p className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
            {agent.provider} · {agent.model}
          </p>
        </div>

        {/* Status badge */}
        <div
          className={cn(
            'flex items-center gap-1.5 px-2.5 py-1 rounded-full',
            'border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2a2a2c]'
          )}
        >
          <div className={cn('w-2 h-2 rounded-full shrink-0', STATUS_COLORS[agent.status])} />
          <span className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
            {STATUS_LABELS[agent.status]}
          </span>
        </div>
      </div>

      {/* Tab bar */}
      <div className={cn('flex gap-1 p-1 rounded-xl', 'bg-black/5 dark:bg-white/5')}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => setActiveTab(tab.id)}
            className={cn(
              'flex-1 rounded-lg py-1.5 text-ui-button font-medium transition-colors',
              activeTab === tab.id
                ? 'bg-white dark:bg-[#3a3a3c] text-foreground-light dark:text-foreground-dark shadow-sm'
                : 'text-secondary-light dark:text-secondary-dark hover:text-foreground-light dark:hover:text-foreground-dark'
            )}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      {activeTab === 'overview' && (
        <div className="flex flex-col gap-4">
          <AssignmentFitCard agent={agent} recentTasks={recentTasks} />

          {/* Stats grid */}
          <div className="grid grid-cols-2 gap-3">
            <StatCard label="Tasks Done" value={String(agent.tasksCompleted)} />
            <StatCard label="In Progress" value={String(agent.tasksInProgress)} />
            <StatCard label="Success Rate" value={`${ratePercent}%`} />
            <StatCard label="Provider" value={agent.provider} />
          </div>

          {/* Agent info */}
          <div
            className={cn(
              'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-3',
              'border border-black/[0.08] dark:border-white/[0.1]',
              'flex flex-col gap-2'
            )}
          >
            <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Details
            </span>
            <div className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-ui-caption">
              <DetailRow
                label="Runtime"
                value={
                  isHostCliAgent(agent)
                    ? `Host CLI · ${agent.cliTool ?? 'unknown'}`
                    : agent.cliTool
                      ? `Container CLI · ${agent.cliTool}`
                      : `${agent.provider} provider`
                }
              />
              <DetailRow label="Status" value={agent.status} />
              <DetailRow
                label="Workspace Access"
                value={agent.workspaceName ?? 'Default workspace'}
              />
              <DetailRow label="Primary Project" value={agent.projectName ?? 'None'} />
              <DetailRow label="Working Directory" value={agent.cwd ?? 'Not applicable'} />
              <DetailRow
                label={isHostCliAgent(agent) ? 'Runtime ID' : 'Container'}
                value={
                  isHostCliAgent(agent)
                    ? (agent.runtimeId ?? 'Pending sidecar')
                    : (agent.containerId?.slice(0, 12) ??
                      (agent.cliTool ? 'Pending' : 'Not applicable'))
                }
              />
            </div>
            <WorkspaceBoundaryNote agent={agent} />
          </div>

          {/* Control panel */}
          <AgentControlPanel agent={agent} onDeleted={onBack} />
        </div>
      )}

      {activeTab === 'tasks' && <AgentTasksTab agentId={agent.id} />}

      {activeTab === 'history' && <ChatView agentId={agent.id} />}

      {activeTab === 'terminal' && agent.cliTool && !isHostCliAgent(agent) && agent.containerId && (
        <Suspense
          fallback={
            <div
              className={cn(
                'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-6',
                'border border-black/[0.08] dark:border-white/[0.1]',
                'text-center text-ui-body text-secondary-light dark:text-secondary-dark'
              )}
            >
              Loading terminal…
            </div>
          }
        >
          <AgentTerminalTab
            agentId={agent.id}
            agentName={agent.name}
            cliTool={agent.cliTool}
            containerId={agent.containerId}
            agentStatus={agent.status}
          />
        </Suspense>
      )}

      {activeTab === 'terminal' &&
        agent.cliTool &&
        !isHostCliAgent(agent) &&
        !agent.containerId && <PendingTerminal agent={agent} />}

      {activeTab === 'plugins' && <AgentPluginsTab agentId={agent.id} />}

      {activeTab === 'config' && <AgentConfigTab agentId={agent.id} />}
    </div>
  )
}

function AssignmentFitCard({
  agent,
  recentTasks,
}: {
  agent: AgentInfo
  recentTasks: TaskSummary[]
}) {
  const available = agent.status === 'idle'
  const activeTask = recentTasks.find((task) => task.state === 'working' || task.state === 'queued')
  const latestTask = recentTasks[0]
  const appliedSkillCount = recentTasks.reduce(
    (sum, task) => sum + (task.contextCounts?.appliedSkills ?? 0),
    0
  )
  const availability = available
    ? 'Can be assigned now'
    : agent.status === 'working'
      ? 'Already working'
      : 'Unavailable until restarted or reconnected'
  const hostCli = isHostCliAgent(agent)
  const runtime = hostCli
    ? `${agent.cliTool ?? 'Host'} CLI on enrolled machine`
    : agent.cliTool
      ? `${agent.cliTool} Container CLI`
      : `${agent.provider} provider`
  const credential = hostCli
    ? 'Uses credentials and tools installed on the enrolled machine.'
    : agent.cliTool === 'codex'
      ? 'Container CLI OAuth status is checked in Runtime settings.'
      : agent.cliTool
        ? 'Container credentials are injected when the agent starts.'
        : 'Provider API key readiness comes from Settings providers.'

  return (
    <section
      data-testid="agent-assignment-fit"
      className={cn(
        'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-3',
        'border border-black/[0.08] dark:border-white/[0.1]',
        'flex flex-col gap-3'
      )}
    >
      <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
            Availability
          </p>
          <h2 className="mt-0.5 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {availability}
          </h2>
        </div>
        <span
          className={cn(
            'inline-flex h-7 w-fit items-center rounded-full px-2.5 text-ui-caption font-medium',
            available
              ? 'bg-apple-green/10 text-apple-green'
              : agent.status === 'working'
                ? 'bg-apple-orange/10 text-apple-orange'
                : 'bg-apple-gray-5 text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
          )}
        >
          {STATUS_LABELS[agent.status]}
        </span>
      </div>

      <div className="grid gap-2 text-ui-caption sm:grid-cols-2">
        <ProfileSummaryRow
          label="Current work"
          value={activeTask?.params.task ?? agent.currentTask ?? 'No active task'}
        />
        <ProfileSummaryRow
          label="Recent update"
          value={
            latestTask
              ? `${latestTask.params.task} updated ${formatRelativeTime(latestTask.updatedAt)}`
              : 'No recent task updates'
          }
        />
        <ProfileSummaryRow label="Runtime" value={runtime} />
        <ProfileSummaryRow
          label="Skills"
          value={
            appliedSkillCount > 0
              ? `${appliedSkillCount} applied skill${appliedSkillCount === 1 ? '' : 's'} in recent work`
              : 'Attach and review skills from task context'
          }
        />
        <ProfileSummaryRow label="Credentials" value={credential} />
      </div>
    </section>
  )
}

function ProfileSummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-lg bg-black/[0.03] px-3 py-2 dark:bg-white/[0.04]">
      <span className="block text-secondary-light dark:text-secondary-dark">{label}</span>
      <span className="mt-0.5 block truncate font-medium text-foreground-light dark:text-foreground-dark">
        {value}
      </span>
    </div>
  )
}

interface StatCardProps {
  label: string
  value: string
}

function StatCard({ label, value }: StatCardProps) {
  return (
    <div
      className={cn(
        'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-3',
        'border border-black/[0.08] dark:border-white/[0.1]',
        'flex flex-col gap-1'
      )}
    >
      <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">{label}</span>
      <span className="text-ui-metric font-semibold text-foreground-light dark:text-foreground-dark">
        {value}
      </span>
    </div>
  )
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <span className="text-secondary-light dark:text-secondary-dark">{label}</span>
      <span className="text-foreground-light dark:text-foreground-dark font-medium truncate">
        {value}
      </span>
    </>
  )
}

function PendingTerminal({ agent }: { agent: AgentInfo }) {
  const { startAgent, error } = useAgentsStore()
  const [starting, setStarting] = useState(false)

  async function handleStart() {
    if (starting || !agent.cliTool) return
    setStarting(true)
    await startAgent(agent.id)
    setStarting(false)
  }

  return (
    <div
      className={cn(
        'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-6',
        'border border-black/[0.08] dark:border-white/[0.1]',
        'flex flex-col items-center gap-3 text-center'
      )}
    >
      <div className="flex flex-col gap-1">
        <span className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          No container is running
        </span>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {agent.cliTool
            ? `${agent.cliTool} is ready to start.`
            : 'This agent has no container CLI.'}
        </span>
      </div>
      {error && (
        <div className="rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red">
          {error}
        </div>
      )}
      {agent.cliTool && (
        <button
          type="button"
          onClick={handleStart}
          disabled={starting}
          className={cn(
            'rounded-full px-4 py-2 text-ui-button font-medium',
            'bg-apple-blue text-white hover:bg-apple-blue/90 transition-colors',
            starting && 'opacity-50 cursor-not-allowed'
          )}
        >
          {starting ? 'Starting…' : 'Start Agent'}
        </button>
      )}
    </div>
  )
}
