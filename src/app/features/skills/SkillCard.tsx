import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { Skill } from '@app/entities/skill'
import { savedInstructionAudienceLabel } from './model/savedInstructionLabels'

interface SkillCardProps {
  skill: Skill
  onClick: (skill: Skill) => void
}

export function SkillCard({ skill, onClick }: SkillCardProps) {
  const statusLabel = skill.installed ? 'Ready to reuse' : 'Check before use'
  const summary =
    skill.description || 'Open details to check the reusable steps before using this skill.'
  const author = skill.pluginAuthor.trim()
  const savedInLabel = savedInstructionCardAudienceLabel(skill.plugin, author)
  return (
    <button
      type="button"
      onClick={() => onClick(skill)}
      aria-label={`${skill.name}. ${statusLabel}. ${summary}`}
      className={cn(
        'w-full rounded-card px-4 py-3 text-left text-ui-button transition-colors',
        'border border-black/[0.08] bg-white hover:bg-black/[0.025]',
        'dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:hover:bg-white/[0.05]',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35'
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <span className="truncate text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {skill.name}
          </span>
          <p className="line-clamp-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {summary}
          </p>
          <span className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {savedInLabel}
          </span>
          {skill.triggerPattern && (
            <span className={cn(uiStyles.chip, 'mt-1 w-fit max-w-full truncate')}>
              {`Matching words: ${skill.triggerPattern}`}
            </span>
          )}
        </div>

        {/* Install status badge */}
        <span className="mt-0.5 inline-flex shrink-0 items-center gap-1.5 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          <span
            className={cn(
              'h-1.5 w-1.5 rounded-full',
              skill.installed ? 'bg-apple-blue' : 'bg-gray-400'
            )}
          />
          {statusLabel}
        </span>
      </div>
    </button>
  )
}

function savedInstructionCardAudienceLabel(source: string, author: string): string {
  const audience = savedInstructionAudienceLabel(source, 'skills')
  return author ? `${audience} by ${author}` : audience
}
