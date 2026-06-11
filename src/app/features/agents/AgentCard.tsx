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
  offline: 'Reconnect before sending work',
}

export function agentCardStatusHelp(
  status: AgentInfo['status'] | string | null | undefined
): string {
  const statusKey = agentStatusKey(status)
  if (!statusKey) return 'Check this agent before sending work'
  return STATUS_HELP[statusKey] ?? 'Check this agent before sending work'
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
  const statusHelp = agentCardStatusHelp(agent.status)
  const runtimeLabel = isHostCliAgent(agent)
    ? 'This computer'
    : agent.cliTool
      ? 'Managed workspace'
      : 'Chat-only agent'
  const projectLabel = agent.projectName ?? agent.workspaceName ?? 'Choose a starting project'
  const serviceLabel = agentServiceLabel(agent)

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
            {agent.currentTask}
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
        <Metric value={String(agent.tasksCompleted)} label="Finished" />
        <Metric value={String(agent.tasksInProgress)} label="Running" />
        <Metric value={`${ratePercent}%`} label="Success" />
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
