import { cn } from '@app/shared/lib/utils'
import type { CliTool } from '@shared/types'
import type { AgentRuntimeKind } from '@app/shared/model/agents.store'

interface AgentKindBadgeProps {
  cliTool?: CliTool
  runtimeKind?: AgentRuntimeKind
  className?: string
}

// Runtime wording mirrors the create/enrollment paths so operators can quickly
// tell whether Docker lifecycle actions apply.
export function AgentKindBadge({ cliTool, runtimeKind, className }: AgentKindBadgeProps) {
  const isHost = runtimeKind === 'host-cli'
  const isContainer = Boolean(cliTool) && !isHost
  const label = isHost ? 'Host CLI' : isContainer ? 'Container' : 'Provider'
  const title = isHost
    ? 'Runs a local CLI through an enrolled sidecar'
    : isContainer
      ? 'Runs the in-container CLI (claude/codex/gemini/opencode)'
      : 'Calls the LLM provider directly — no container'

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
