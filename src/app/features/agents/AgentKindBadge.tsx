import { cn } from '@app/shared/lib/utils'
import type { CliTool } from '@shared/types'
import {
  runtimeKindDescription,
  runtimeKindShortLabel,
  type AgentRuntimeKind,
} from '@app/entities/agent'

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
  const kind = isHost ? 'cli' : isContainer ? 'container' : 'api'
  const label = runtimeKindShortLabel(kind)
  const title = runtimeKindDescription(kind)

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
