import { useTranslation } from 'react-i18next'
import { X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { Skill } from '@app/shared/model/skills.store'

interface SkillDetailModalProps {
  skill: Skill
  onClose: () => void
}

export function SkillDetailModal({ skill, onClose }: SkillDetailModalProps) {
  const { t } = useTranslation()
  // Derive a rough version from the marketplace field or show fallback
  const version = skill.marketplace ? `${skill.marketplace}` : t('skills.detail.versionLatest')
  const author = skill.pluginAuthor || t('skills.detail.unknownAuthor')

  function handleBackdropClick(e: React.MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose()
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-end sm:items-center justify-center p-4 bg-black/40 backdrop-blur-sm"
      onClick={handleBackdropClick}
      role="dialog"
      aria-modal="true"
      aria-label={skill.name}
    >
      <div
        className={cn(
          'w-full max-w-md max-h-[80vh] overflow-y-auto',
          'rounded-card border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2c2c2e]',
          'flex flex-col'
        )}
      >
        {/* Header */}
        <div className="flex items-start justify-between gap-4 px-5 pt-5 pb-4 border-b border-black/[0.06] dark:border-white/[0.06]">
          <div className="flex flex-col gap-0.5 min-w-0">
            <h2 className="text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
              {skill.name}
            </h2>
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {skill.plugin} · {author} · {version}
            </p>
          </div>

          <button
            type="button"
            onClick={onClose}
            aria-label={t('skills.detail.closeAria')}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-secondary-light transition-colors hover:bg-black/[0.04] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:text-secondary-dark dark:hover:bg-white/[0.06]"
          >
            <X size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        </div>

        {/* Body */}
        <div className="flex flex-col gap-4 px-5 py-4">
          {/* Status + Container CLI (see docs/architecture/glossary.md) */}
          <div className="flex items-center gap-2">
            <span className={cn(skill.installed ? uiStyles.activeBadge : uiStyles.badge)}>
              {skill.installed
                ? t('skills.detail.statusInstalled')
                : t('skills.detail.statusNotInstalled')}
            </span>
            {skill.cliTool && (
              <span
                className={uiStyles.activeBadge}
                title={t('skills.detail.containerCliTooltip', { tool: skill.cliTool })}
              >
                {skill.cliTool}
              </span>
            )}
          </div>

          {/* Description */}
          {skill.description && (
            <div className="flex flex-col gap-1">
              <span className="text-ui-caption font-semibold uppercase text-secondary-light dark:text-secondary-dark">
                {t('skills.detail.descriptionHeading')}
              </span>
              <p className="text-ui-body text-foreground-light dark:text-foreground-dark">
                {skill.description}
              </p>
            </div>
          )}

          {/* Skill content / README preview */}
          {skill.content && (
            <div className="flex flex-col gap-1">
              <span className="text-ui-caption font-semibold uppercase text-secondary-light dark:text-secondary-dark">
                {t('skills.detail.detailsHeading')}
              </span>
              <pre
                className={cn(
                  'whitespace-pre-wrap font-mono text-ui-caption',
                  'text-foreground-light dark:text-foreground-dark',
                  'rounded-card bg-black/[0.04] p-3 dark:bg-white/[0.04]',
                  'max-h-40 overflow-y-auto'
                )}
              >
                {skill.content}
              </pre>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-5 pb-5 pt-2 flex justify-end">
          <button type="button" onClick={onClose} className={uiStyles.secondaryButton}>
            {t('skills.detail.close')}
          </button>
        </div>
      </div>
    </div>
  )
}
