import { useTranslation } from 'react-i18next'
import { X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { Skill } from '@app/shared/model/skills.store'
import { knownWorkToolLabel, savedInstructionSourceLabel } from './model/savedInstructionLabels'

interface SkillDetailModalProps {
  skill: Skill
  onClose: () => void
}

export function SkillDetailModal({ skill, onClose }: SkillDetailModalProps) {
  const { t } = useTranslation()
  const availability = skill.marketplace
    ? skillAvailabilityLabel(skill.marketplace, (key) => t(key))
    : t('skills.detail.availabilityLatest')
  const author = skill.pluginAuthor || t('skills.detail.unknownAuthor')
  const source = skill.plugin
    ? savedInstructionSourceLabel(skill.plugin, t('skills.detail.unknownSource'))
    : t('skills.detail.unknownSource')
  const toolLabel = skill.cliTool ? knownWorkToolLabel(skill.cliTool) : null
  const cliLabel = skill.cliTool
    ? toolLabel
      ? t('skills.detail.cliFit', { tool: toolLabel })
      : t('skills.detail.unknownToolFit')
    : t('skills.detail.allAgentsFit')

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
              {t('skills.detail.subtitle')}
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
          <div className="flex flex-wrap items-center gap-2">
            <span className={cn(skill.installed ? uiStyles.activeBadge : uiStyles.badge)}>
              {skill.installed
                ? t('skills.detail.statusReady')
                : t('skills.detail.statusNeedsInstall')}
            </span>
            <span
              className={uiStyles.badge}
              title={
                skill.cliTool
                  ? toolLabel
                    ? t('skills.detail.containerCliTooltip', { tool: toolLabel })
                    : t('skills.detail.unknownToolTooltip')
                  : t('skills.detail.allAgentsTooltip')
              }
            >
              {cliLabel}
            </span>
          </div>

          <section className="rounded-card border border-apple-blue/20 bg-apple-blue/[0.06] px-3 py-2">
            <h3 className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
              {t('skills.detail.nextStepHeading')}
            </h3>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {skill.installed
                ? t('skills.detail.nextStepReady')
                : t('skills.detail.nextStepNeedsInstall')}
            </p>
          </section>

          <div className="grid gap-2 sm:grid-cols-3">
            <SkillMeta label={t('skills.detail.sourceLabel')} value={source} />
            <SkillMeta label={t('skills.detail.authorLabel')} value={author} />
            <SkillMeta label={t('skills.detail.availabilityLabel')} value={availability} />
          </div>

          <section className="flex flex-col gap-1">
            <h3 className="text-ui-caption font-semibold uppercase text-secondary-light dark:text-secondary-dark">
              {t('skills.detail.descriptionHeading')}
            </h3>
            <p className="text-ui-body text-foreground-light dark:text-foreground-dark">
              {skill.description || t('skills.detail.noDescription')}
            </p>
          </section>

          {skill.triggerPattern && (
            <section className="flex flex-col gap-1 rounded-card border border-black/[0.08] px-3 py-2 dark:border-white/[0.08]">
              <h3 className="text-ui-caption font-semibold text-secondary-light dark:text-secondary-dark">
                {t('skills.detail.triggerHeading')}
              </h3>
              <p className="text-ui-body text-foreground-light dark:text-foreground-dark">
                {t('skills.detail.triggerHelper')}
              </p>
              <span className="mt-1 w-fit max-w-full rounded-full bg-black/[0.04] px-2 py-0.5 text-ui-caption text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
                {skill.triggerPattern}
              </span>
            </section>
          )}

          <section className="flex flex-col gap-1">
            <h3 className="text-ui-caption font-semibold uppercase text-secondary-light dark:text-secondary-dark">
              {t('skills.detail.detailsHeading')}
            </h3>
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {t('skills.detail.detailsHelper')}
            </p>
            {skill.content ? (
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
            ) : (
              <div className={uiStyles.note}>{t('skills.detail.noContent')}</div>
            )}
          </section>
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

function SkillMeta({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-card bg-black/[0.025] px-3 py-2 dark:bg-white/[0.04]">
      <div className="text-ui-caption text-secondary-light dark:text-secondary-dark">{label}</div>
      <div className="mt-1 truncate text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
        {value}
      </div>
    </div>
  )
}

function skillAvailabilityLabel(value: string, translate: (key: string) => string): string {
  switch (value.trim().toLowerCase()) {
    case 'workspace':
      return translate('skills.detail.availabilityWorkspace')
    case 'global':
      return translate('skills.detail.availabilityGlobal')
    case 'project':
      return translate('skills.detail.availabilityProject')
    default:
      return translate('skills.detail.availabilityNeedsReview')
  }
}
