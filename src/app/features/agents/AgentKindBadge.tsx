import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
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
  const kind = isHost ? 'cli' : cliTool ? 'container' : 'api'
  const label = runtimeKindShortLabel(kind)
  const title = runtimeKindDescription(kind)

  return (
    <span className={cn(uiStyles.chip, 'shrink-0', className)} title={title}>
      {label}
    </span>
  )
}
