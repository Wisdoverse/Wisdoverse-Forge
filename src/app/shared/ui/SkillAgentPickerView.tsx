import { useTranslation } from 'react-i18next'
import { Plus, Unlink } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'

export interface SkillAgentOption {
  id: string
  name: string
}

export interface LinkedSkillAgent {
  agentId: string
  name: string
}

interface SkillAgentPickerViewProps {
  /** Agents that can still be attached. */
  available: SkillAgentOption[]
  /** Agents already following the skill. */
  linked: LinkedSkillAgent[]
  selectedAgentId: string
  busy: boolean
  error: string | null
  detachingId: string | null
  onSelect: (agentId: string) => void
  onAttach: (event: React.FormEvent) => void
  onDetach: (agentId: string) => void
  testId?: string
}

/**
 * Presentational attach/detach control for a skill. Data wiring stays with the
 * owning feature (skills page, skill draft flow) so shared stays layer-pure.
 */
export function SkillAgentPickerView({
  available,
  linked,
  selectedAgentId,
  busy,
  error,
  detachingId,
  onSelect,
  onAttach,
  onDetach,
  testId = 'skill-agent-picker',
}: SkillAgentPickerViewProps) {
  const { t } = useTranslation()
  const selectId = `skill-agent-select-${testId}`

  return (
    <div className="flex flex-col gap-2" data-testid={testId}>
      <div className="flex flex-wrap items-center gap-1.5">
        {linked.length === 0 ? (
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {t('skillAgents.empty')}
          </p>
        ) : (
          linked.map((agent) => (
            <span
              key={agent.agentId}
              className={cn(
                'inline-flex items-center gap-1.5 rounded-button bg-apple-blue/10 px-2 py-1',
                'text-ui-caption font-medium text-apple-blue'
              )}
            >
              {agent.name || t('skillAgents.unnamedAgent')}
              <button
                type="button"
                aria-label={t('skillAgents.detachAria', { name: agent.name })}
                disabled={detachingId === agent.agentId}
                onClick={() => onDetach(agent.agentId)}
                className="rounded-full p-0.5 transition-colors hover:bg-apple-blue/20 disabled:cursor-wait disabled:opacity-60"
              >
                <Unlink size={12} strokeWidth={2.25} aria-hidden="true" />
              </button>
            </span>
          ))
        )}
      </div>

      <form onSubmit={onAttach} className="flex flex-wrap items-center gap-2">
        <label htmlFor={selectId} className="sr-only">
          {t('skillAgents.selectLabel')}
        </label>
        <select
          id={selectId}
          value={selectedAgentId}
          onChange={(event) => onSelect(event.target.value)}
          className={cn(
            'min-w-40 rounded-button border border-black/[0.1] bg-white px-2 py-1.5 text-ui-caption',
            'text-foreground-light focus-visible:outline-2 focus-visible:outline-[rgb(var(--ring))]',
            'dark:border-white/[0.12] dark:bg-surface-dark dark:text-foreground-dark'
          )}
        >
          <option value="">{t('skillAgents.selectPlaceholder')}</option>
          {available.map((agent) => (
            <option key={agent.id} value={agent.id}>
              {agent.name || t('skillAgents.unnamedAgent')}
            </option>
          ))}
        </select>
        <button
          type="submit"
          disabled={!selectedAgentId || busy || available.length === 0}
          className="inline-flex items-center gap-1.5 rounded-button bg-apple-blue px-2.5 py-1.5 text-ui-caption font-semibold text-white transition-colors hover:bg-apple-blue/90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Plus size={13} strokeWidth={2.25} aria-hidden="true" />
          {busy ? t('skillAgents.attaching') : t('skillAgents.attach')}
        </button>
      </form>

      {error && (
        <p role="alert" aria-live="polite" className="text-ui-caption font-medium text-apple-red">
          {error}
        </p>
      )}
    </div>
  )
}
