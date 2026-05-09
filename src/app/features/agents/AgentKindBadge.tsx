import { cn } from '@app/shared/lib/utils'
import type { CliTool } from '@shared/types'

interface AgentKindBadgeProps {
  cliTool?: CliTool
  className?: string
}

// Container-backed CLI agents are runtime-heavy (docker image, terminal, sidecar);
// provider+prompt agents are just LLM calls. Wording matches the CreateAgentModal
// radio so the same term appears everywhere.
export function AgentKindBadge({ cliTool, className }: AgentKindBadgeProps) {
  const isContainer = Boolean(cliTool)
  return (
    <span
      className={cn(
        'shrink-0 rounded-full border px-2 py-0.5 text-ui-caption font-normal',
        isContainer
          ? 'border-apple-blue/20 bg-white text-apple-blue dark:bg-white/[0.04]'
          : 'border-black/[0.08] bg-white text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark',
        className
      )}
      title={
        isContainer
          ? 'Runs the in-container CLI (claude/codex/gemini/opencode)'
          : 'Calls the LLM provider directly — no container'
      }
    >
      {isContainer ? 'Container' : 'Provider'}
    </span>
  )
}
