import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { RuntimeType, CliTool } from '@app/shared/api/legacy/settingsApi'

// ============================================================================
// Setting Row
// ============================================================================

interface SettingRowProps {
  label: string
  description?: string
  children: React.ReactNode
}

function SettingRow({ label, description, children }: SettingRowProps) {
  return (
    <div className={cn('flex items-center justify-between gap-4 px-4 py-3', uiStyles.row)}>
      <div className="min-w-0">
        <span className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          {label}
        </span>
        {description && (
          <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {description}
          </p>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  )
}

// ============================================================================
// RuntimeSection
// ============================================================================

export function RuntimeSection() {
  const { t } = useTranslation()
  const {
    runtimeSettings,
    runtimeLoading,
    runtimeError,
    loadRuntimeSettings,
    updateRuntimeSettings,
  } = useSettingsStore()
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    void loadRuntimeSettings()
  }, [loadRuntimeSettings])

  async function handleRuntimeChange(value: RuntimeType) {
    if (!runtimeSettings) return
    setSaving(true)
    await updateRuntimeSettings({ defaultRuntime: value })
    setSaving(false)
  }

  async function handleCliToolChange(value: CliTool) {
    if (!runtimeSettings) return
    setSaving(true)
    await updateRuntimeSettings({ defaultCliTool: value })
    setSaving(false)
  }

  const runtimeLabel = (rt: RuntimeType): string =>
    t(`settings.runtime.runtimeLabels.${rt}`, { defaultValue: rt })
  const cliToolLabel = (tool: CliTool): string =>
    t(`settings.runtime.cliToolLabels.${tool}`, { defaultValue: tool })

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>{t('settings.runtime.title')}</h2>
          <p className={uiStyles.sectionDescription}>{t('settings.runtime.description')}</p>
        </div>
        {saving && (
          <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {t('settings.runtime.saving')}
          </span>
        )}
      </div>

      {/* Error */}
      {runtimeError && <div className={uiStyles.error}>{runtimeError}</div>}

      {/* Settings card */}
      <div className={uiStyles.card}>
        {runtimeLoading && !runtimeSettings ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            {t('settings.runtime.loading')}
          </div>
        ) : !runtimeSettings ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              {t('settings.runtime.couldNotLoad')}
            </p>
          </div>
        ) : (
          <>
            {/* Default Runtime */}
            <SettingRow
              label={t('settings.runtime.defaultRuntimeLabel')}
              description={t('settings.runtime.defaultRuntimeDescription')}
            >
              <select
                value={runtimeSettings.defaultRuntime}
                onChange={(e) => handleRuntimeChange(e.target.value as RuntimeType)}
                disabled={saving}
                className={cn(uiStyles.select, 'disabled:opacity-50')}
              >
                {runtimeSettings.availableRuntimes.map((rt) => (
                  <option key={rt} value={rt}>
                    {runtimeLabel(rt)}
                  </option>
                ))}
              </select>
            </SettingRow>

            {/* Default Container CLI */}
            <SettingRow
              label={t('settings.runtime.defaultContainerCliLabel')}
              description={t('settings.runtime.defaultContainerCliDescription')}
            >
              <select
                value={runtimeSettings.defaultCliTool}
                onChange={(e) => handleCliToolChange(e.target.value as CliTool)}
                disabled={saving}
                className={cn(uiStyles.select, 'disabled:opacity-50')}
              >
                {runtimeSettings.availableCliTools.map((tool) => (
                  <option key={tool} value={tool}>
                    {cliToolLabel(tool)}
                  </option>
                ))}
              </select>
            </SettingRow>

            {/* Read-only: Available Runtimes */}
            <SettingRow
              label={t('settings.runtime.availableRuntimesLabel')}
              description={t('settings.runtime.availableRuntimesDescription')}
            >
              <div className="flex flex-wrap justify-end gap-1.5">
                {runtimeSettings.availableRuntimes.map((rt) => (
                  <span key={rt} className={uiStyles.badge}>
                    {rt}
                  </span>
                ))}
              </div>
            </SettingRow>

            {/* Read-only: Available Container CLIs */}
            <SettingRow
              label={t('settings.runtime.availableContainerClisLabel')}
              description={t('settings.runtime.availableContainerClisDescription')}
            >
              <div className="flex flex-wrap justify-end gap-1.5">
                {runtimeSettings.availableCliTools.map((tool) => (
                  <span key={tool} className={uiStyles.badge}>
                    {tool}
                  </span>
                ))}
              </div>
            </SettingRow>
          </>
        )}
      </div>
    </div>
  )
}
