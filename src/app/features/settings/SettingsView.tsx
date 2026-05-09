import { useTheme } from '@app/shared/model/theme.context'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'

export function SettingsView() {
  const { theme, toggleTheme } = useTheme()

  return (
    <div className="mx-auto h-full max-w-2xl overflow-y-auto px-4 py-5 sm:px-6">
      <h1 className="mb-6 text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
        Settings
      </h1>

      {/* Appearance section */}
      <section className="mb-6">
        <h2 className={uiStyles.groupLabel}>Appearance</h2>
        <div className={cn(uiStyles.card)}>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-foreground-light dark:text-foreground-dark">
              Theme
            </span>
            <button type="button" onClick={toggleTheme} className={uiStyles.secondaryButton}>
              {theme === 'light' ? 'Switch to Dark' : 'Switch to Light'}
            </button>
          </div>
        </div>
      </section>

      {/* About section */}
      <section>
        <h2 className={uiStyles.groupLabel}>About</h2>
        <div className={cn(uiStyles.card, 'divide-y divide-[rgb(var(--border))]')}>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-foreground-light dark:text-foreground-dark">
              Application
            </span>
            <span className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Wisdoverse Forge
            </span>
          </div>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-foreground-light dark:text-foreground-dark">
              Version
            </span>
            <span className="text-ui-body text-secondary-light dark:text-secondary-dark">
              0.1.0
            </span>
          </div>
        </div>
      </section>
    </div>
  )
}
