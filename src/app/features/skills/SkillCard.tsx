import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { Skill } from '@app/shared/model/skills.store'

interface SkillCardProps {
  skill: Skill
  onClick: (skill: Skill) => void
}

export function SkillCard({ skill, onClick }: SkillCardProps) {
  return (
    <button
      type="button"
      onClick={() => onClick(skill)}
      className={cn(
        'w-full rounded-lg px-4 py-3 text-left text-ui-button transition-colors',
        'border border-black/[0.08] bg-white hover:border-apple-blue/35 hover:bg-white',
        'dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:hover:border-apple-blue/35 dark:hover:bg-white/[0.05]',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35'
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <span className="truncate text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {skill.name}
          </span>
          <p className="line-clamp-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {skill.description || 'No description available'}
          </p>
          <span className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {skill.plugin}
            {skill.pluginAuthor ? ` · ${skill.pluginAuthor}` : ''}
          </span>
        </div>

        {/* Install status badge */}
        <span
          className={cn('mt-0.5 shrink-0', skill.installed ? uiStyles.activeBadge : uiStyles.badge)}
        >
          {skill.installed ? 'Installed' : 'Available'}
        </span>
      </div>
    </button>
  )
}
