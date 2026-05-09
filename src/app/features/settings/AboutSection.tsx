import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'

const REPO_URL = 'https://github.com/Wisdoverse/wisdoverse-forge'

export function AboutSection() {
  return (
    <div className="space-y-6" data-testid="settings-about">
      <div>
        <h2 className={uiStyles.sectionTitle}>About</h2>
        <p className={uiStyles.sectionDescription}>Build info for this Wisdoverse Forge install</p>
      </div>

      <div>
        <h3 className={uiStyles.groupLabel}>Application</h3>
        <div className={cn(uiStyles.card, 'divide-y divide-[rgb(var(--border))]')}>
          <Row label="Name" value="Wisdoverse Forge" />
          <Row label="Version" value={__APP_VERSION__} valueTestId="settings-about-version" />
          <Row
            label="Source"
            value={
              <a
                href={REPO_URL}
                target="_blank"
                rel="noreferrer"
                className="text-apple-blue hover:underline"
              >
                github.com/Wisdoverse/wisdoverse-forge
              </a>
            }
          />
        </div>
      </div>
    </div>
  )
}

function Row({
  label,
  value,
  valueTestId,
}: {
  label: string
  value: React.ReactNode
  valueTestId?: string
}) {
  return (
    <div className="flex items-center justify-between px-4 py-3">
      <span className="text-ui-body text-secondary-light dark:text-secondary-dark">{label}</span>
      <span
        data-testid={valueTestId}
        className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark"
      >
        {value}
      </span>
    </div>
  )
}
