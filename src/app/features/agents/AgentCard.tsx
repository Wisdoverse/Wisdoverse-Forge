import { cn } from '@app/shared/lib/utils'
import {
  agentAvatarInitial,
  agentServiceLabel,
  agentStatusKey,
  agentStatusLabel,
  isHostCliAgent,
  type AgentInfo,
} from '@app/entities/agent'
import { AgentKindBadge } from './AgentKindBadge'

const PROVIDER_GRADIENTS: Record<string, string> = {
  Anthropic: 'bg-[#f5f5f7] text-[#1d1d1f] dark:bg-white/[0.08] dark:text-white',
  Google: 'bg-[#f5f5f7] text-[#1d1d1f] dark:bg-white/[0.08] dark:text-white',
  OpenAI: 'bg-[#f5f5f7] text-[#1d1d1f] dark:bg-white/[0.08] dark:text-white',
}

const STATUS_COLORS: Record<string, string> = {
  working: 'bg-[#1d1d1f] dark:bg-white',
  idle: 'bg-[#7a7a7a]',
  offline: 'bg-[#d2d2d7]',
}

const STATUS_FALLBACK_COLOR = 'bg-[#d2d2d7]'

const STATUS_HELP: Record<string, string> = {
  working: 'Running a task now',
  idle: 'Ready for a new task',
  offline: 'Open this agent to reconnect before work',
}

export function agentCardStatusHelp(
  status: AgentInfo['status'] | string | null | undefined,
  agent?: AgentInfo
): string {
  const statusKey = agentStatusKey(status)
  if (!statusKey) {
    return agent && !agent.cliTool
      ? 'Check this agent before sending a message'
      : 'Check this agent before sending work'
  }
  if (statusKey === 'offline' && agent) {
    if (isHostCliAgent(agent)) {
      return 'Open this agent to see the reconnect steps from Agents.'
    }
    if (agent.cliTool) {
      return 'Open this agent and start project files before sending Tasks or code changes.'
    }
    return 'Open this agent and check its AI service before sending a message.'
  }
  if (statusKey === 'working' && agent && !agent.cliTool) return 'Answering a message now'
  if (statusKey === 'idle' && agent && !agent.cliTool) {
    return 'Ready for direct chat. Use Project files or This computer for Tasks and code changes.'
  }
  return (
    STATUS_HELP[statusKey] ??
    (agent && !agent.cliTool
      ? 'Check this agent before sending a message'
      : 'Check this agent before sending work')
  )
}

function defaultGradient(provider: string): string {
  return (
    PROVIDER_GRADIENTS[provider] ??
    'bg-[#f5f5f7] text-[#1d1d1f] dark:bg-white/[0.08] dark:text-white'
  )
}

interface AgentCardProps {
  agent: AgentInfo
  onClick?: () => void
}

export function AgentCard({ agent, onClick }: AgentCardProps) {
  const ratePercent = Math.round(agent.successRate * 100)
  const statusKey = agentStatusKey(agent.status)
  const statusLabel = agentStatusLabel(agent.status)
  const statusHelp = agentCardStatusHelp(agent.status, agent)
  const runtimeLabel = isHostCliAgent(agent)
    ? 'This computer'
    : agent.cliTool
      ? 'Project files'
      : 'Simple chat agent'
  const projectLabel = agent.cliTool
    ? (agent.projectName ?? agent.workspaceName ?? 'Open project settings first.')
    : agent.projectName
      ? `Shown in ${agent.projectName}`
      : agent.workspaceName
        ? `Shown in ${agent.workspaceName}`
        : 'No project files needed'
  const serviceLabel = agentServiceLabel(agent)
  const currentWorkLabel = agent.cliTool ? agent.currentTask : 'Answering in Chat'
  const metrics = agent.cliTool
    ? [
        { value: String(agent.tasksCompleted), label: 'Finished' },
        { value: String(agent.tasksInProgress), label: 'Running' },
        { value: `${ratePercent}%`, label: 'Success' },
      ]
    : [
        { value: String(agent.tasksCompleted), label: 'Answered' },
        { value: String(agent.tasksInProgress), label: 'Replying' },
        { value: `${ratePercent}%`, label: 'Answer success' },
      ]

  return (
    <button
      type="button"
      data-testid={`agent-card-${agent.id}`}
      onClick={onClick}
      aria-label={`Open ${agent.name}`}
      className={cn(
        'group flex w-full min-w-0 items-start gap-3 rounded-lg border px-4 py-3 text-left text-ui-button',
        'border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2a2a2c]',
        'transition-colors hover:border-apple-blue/35 hover:bg-white dark:hover:border-apple-blue/35 dark:hover:bg-white/[0.05]',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35'
      )}
    >
      <div
        className={cn(
          'flex h-10 w-10 shrink-0 items-center justify-center rounded-lg',
          'border border-black/[0.08] text-ui-body font-semibold dark:border-white/[0.1]',
          defaultGradient(agent.provider)
        )}
        aria-hidden="true"
      >
        {agentAvatarInitial(agent)}
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <span className="min-w-0 truncate text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {agent.name}
          </span>
          <AgentKindBadge cliTool={agent.cliTool} runtimeKind={agent.runtimeKind} />
        </div>

        <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          <span className="truncate" translate="no">
            {serviceLabel}
          </span>
          <span className="hidden sm:inline" aria-hidden="true">
            ·
          </span>
          <span className="truncate">{runtimeLabel}</span>
          <span className="hidden sm:inline" aria-hidden="true">
            ·
          </span>
          <span className="truncate">{projectLabel}</span>
        </div>

        {agent.currentTask && (
          <p className="mt-2 truncate rounded-md bg-apple-blue/[0.06] px-2 py-1 text-ui-caption text-secondary-light dark:bg-white/[0.05] dark:text-secondary-dark">
            {currentWorkLabel}
          </p>
        )}
        <p
          data-testid={`agent-status-help-${agent.id}`}
          className={cn(
            'mt-2 rounded-md px-2 py-1 text-ui-caption',
            statusKey === 'offline'
              ? 'bg-apple-red/10 text-apple-red'
              : statusKey === 'working'
                ? 'bg-apple-blue/10 text-secondary-light dark:text-secondary-dark'
                : 'bg-apple-green/10 text-secondary-light dark:text-secondary-dark'
          )}
        >
          {statusHelp}
        </p>
      </div>

      <div className="hidden shrink-0 grid-cols-3 gap-3 text-right sm:grid">
        {metrics.map((metric) => (
          <Metric key={metric.label} value={metric.value} label={metric.label} />
        ))}
      </div>

      <span
        data-testid={`agent-status-${agent.id}`}
        className={cn(
          'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border px-2 text-ui-caption font-medium',
          'border-black/[0.08] bg-white text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark'
        )}
      >
        <span
          className={cn('h-2 w-2 rounded-full', STATUS_COLORS[statusKey] ?? STATUS_FALLBACK_COLOR)}
        />
        {statusLabel}
      </span>
    </button>
  )
}

function Metric({ value, label }: { value: string; label: string }) {
  return (
    <span className="min-w-10">
      <span className="block text-ui-body font-semibold tabular-nums text-foreground-light dark:text-foreground-dark">
        {value}
      </span>
      <span className="mt-0.5 block text-ui-caption uppercase text-secondary-light dark:text-secondary-dark">
        {label}
      </span>
    </span>
  )
}
