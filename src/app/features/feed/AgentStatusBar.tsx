import { cn } from '@app/shared/lib/utils'
import type { AgentStatus } from '@app/shared/model/feed.store'

const STATUS_COLORS: Record<string, string> = {
  working: 'bg-apple-green',
  idle: 'bg-apple-gray-2',
  blocked: 'bg-apple-orange',
  offline: 'bg-apple-gray-3',
}

const STATUS_COPY: Record<
  AgentStatus['status'],
  { label: string; description: string; container: string }
> = {
  working: {
    label: 'Working now',
    description: 'This agent is actively handling a task.',
    container: 'bg-apple-green/8',
  },
  idle: {
    label: 'Ready',
    description: 'This agent is connected and waiting for work.',
    container: 'bg-black/[0.04] dark:bg-white/[0.06]',
  },
  blocked: {
    label: 'Needs help',
    description: 'This agent is waiting for someone to clear a blocker.',
    container: 'bg-apple-red/8',
  },
  offline: {
    label: 'Not connected',
    description: 'This agent is not reachable right now.',
    container: 'bg-black/[0.04] dark:bg-white/[0.06]',
  },
}

export function AgentStatusBar({ agents }: { agents: AgentStatus[] }) {
  if (agents.length === 0) return null

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
          </div>
        )
      })}
    </div>
  )
}
