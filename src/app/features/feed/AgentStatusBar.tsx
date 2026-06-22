import { cn } from '@app/shared/lib/utils'
import { LOCAL_AGENT_SETUP_WINDOW_LABEL } from '@app/entities/agent'
import type { AgentStatus } from '@app/shared/model/feed.store'

const STATUS_COLORS: Record<string, string> = {
  working: 'bg-apple-green',
  idle: 'bg-apple-gray-2',
  blocked: 'bg-apple-orange',
  offline: 'bg-apple-gray-3',
}

const STATUS_COPY: Record<
  AgentStatus['status'],
  { label: string; description: string; visibleDetail: string; container: string }
> = {
  working: {
    label: 'Working now',
    description: 'This agent is actively handling a task.',
    visibleDetail: 'Handling a task',
    container: 'bg-apple-green/8',
  },
  idle: {
    label: 'Ready',
    description: 'This agent is connected and waiting for work.',
    visibleDetail: 'Waiting for work',
    container: 'bg-black/[0.04] dark:bg-white/[0.06]',
  },
  blocked: {
    label: 'Needs help',
    description: 'This agent is waiting for help before it can continue.',
    visibleDetail: 'Waiting for help',
    container: 'bg-apple-red/8',
  },
  offline: {
    label: 'Not connected',
    description: 'Open Agents and choose Connect this computer.',
    visibleDetail: 'Start it in Agents',
    container: 'bg-black/[0.04] dark:bg-white/[0.06]',
  },
}

export function AgentStatusBar({ agents }: { agents: AgentStatus[] }) {
  if (agents.length === 0) {
    return (
      <div
        data-testid="agent-status-empty"
        aria-labelledby="agent-status-empty-title"
        className="rounded-lg bg-black/[0.035] px-3 py-2.5 text-[10px] leading-relaxed text-secondary-light dark:bg-white/[0.05] dark:text-secondary-dark"
      >
        <p
          id="agent-status-empty-title"
          className="text-[11px] font-medium text-foreground-light dark:text-foreground-dark"
        >
          Connect an agent before sending work
        </p>
        <ol className="mt-1.5 list-decimal space-y-1 pl-4">
          <li>Open Agents and choose New agent if none exists.</li>
          <li>
            Open {LOCAL_AGENT_SETUP_WINDOW_LABEL}, paste the setup text, and leave that window open.
          </li>
          <li>If an agent already exists, choose Start in Agents.</li>
        </ol>
        <p className="mt-1.5 text-[10px] text-secondary-light dark:text-secondary-dark">
          Success looks like one agent listed here as Ready or Working now.
        </p>
      </div>
    )
  }

  return (
    <div data-testid="agent-status-bar" className="flex items-center gap-2 flex-wrap">
      {agents.map((agent) => {
        const status = STATUS_COPY[agent.status]
        return (
          <div
            key={agent.id}
            aria-label={`${agent.name}: ${status.label}. ${status.description}`}
            className={cn(
              'flex items-center gap-1.5 px-2 py-1 rounded-lg text-[10px]',
              status.container
            )}
          >
            <div className={cn('w-1.5 h-1.5 rounded-full', STATUS_COLORS[agent.status])} />
            <span className="font-medium">{agent.name}</span>
            <span className="text-secondary-light dark:text-secondary-dark">{status.label}</span>
            <span className="text-secondary-light dark:text-secondary-dark">
              {status.visibleDetail}
            </span>
          </div>
        )
      })}
    </div>
  )
}
