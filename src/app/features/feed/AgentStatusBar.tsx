import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { AgentStatus } from '@app/entities/feed'

const STATUS_COLORS: Record<string, string> = {
  working: 'bg-apple-green',
  idle: 'bg-apple-gray-2',
  blocked: 'bg-apple-orange',
  offline: 'bg-apple-gray-3',
}

const STATUS_COPY: Record<
  AgentStatus['status'],
  { label: string; description: string; visibleDetail: string }
> = {
  working: {
    label: 'Working now',
    description: 'This agent is actively handling a task.',
    visibleDetail: 'Handling a task',
  },
  idle: {
    label: 'Ready',
    description: 'This agent is connected and waiting for work.',
    visibleDetail: 'Waiting for work',
  },
  blocked: {
    label: 'Needs help',
    description: 'This agent is waiting for help before it can continue.',
    visibleDetail: 'Waiting for help',
  },
  offline: {
    label: 'Not connected',
    description: 'Open Agents and choose Connect this computer.',
    visibleDetail: 'Start it in Agents',
  },
}

export function AgentStatusBar({ agents }: { agents: AgentStatus[] }) {
  if (agents.length === 0) {
    return (
      <div
        data-testid="agent-status-empty"
        aria-labelledby="agent-status-empty-title"
        className={cn(uiStyles.note, 'py-2.5 text-ui-caption leading-relaxed')}
      >
        <p
          id="agent-status-empty-title"
          className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark"
        >
          Connect an agent before sending work
        </p>
        <ol className="mt-1.5 list-decimal space-y-1 pl-4">
          <li>Open Agents and choose New agent if none exists.</li>
          <li>If an agent already exists, choose Start in Agents.</li>
          <li>After creating or starting one, come back here and wait for Ready or Working now.</li>
        </ol>
        <p className="mt-1.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
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
            className="flex items-center gap-1.5 text-ui-body"
          >
            <div
              className={cn('h-1.5 w-1.5 rounded-full', STATUS_COLORS[agent.status])}
              aria-hidden="true"
            />
            <span className="font-medium text-foreground-light dark:text-foreground-dark">
              {agent.name}
            </span>
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
