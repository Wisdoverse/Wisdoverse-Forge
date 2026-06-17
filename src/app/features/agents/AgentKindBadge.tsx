import { cn } from '@app/shared/lib/utils'
import type { CliTool } from '@shared/types'
import type { AgentRuntimeKind } from '@app/entities/agent'

interface AgentKindBadgeProps {
  cliTool?: CliTool
  runtimeKind?: AgentRuntimeKind
  className?: string
}

// Badge titles describe the user's choice, while protocol names stay inside
// the API/store layer.
export function AgentKindBadge({ cliTool, runtimeKind, className }: AgentKindBadgeProps) {
  const isHost = runtimeKind === 'cli'
  const isContainer = Boolean(cliTool) && !isHost
  const label = isHost ? 'This computer' : isContainer ? 'Managed workspace' : 'Chat-only'
  const title = isHost
    ? 'Uses files and tools on this connected computer. Use it when work should stay there.'
    : isContainer
      ? 'Works in a Forge project area. It can change files, run checks, and save what it checked.'
      : 'Answers in chat through a connected AI service. It cannot open project files on its own.'

  return (
    <span
      className={cn(
        'shrink-0 rounded-full border px-2 py-0.5 text-ui-caption font-normal',
        isHost
          ? 'border-apple-green/20 bg-white text-apple-green dark:bg-white/[0.04]'
          : isContainer
            ? 'border-apple-blue/20 bg-white text-apple-blue dark:bg-white/[0.04]'
            : 'border-black/[0.08] bg-white text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark',
        className
      )}
      title={title}
    >
      {label}
    </span>
  )
}
