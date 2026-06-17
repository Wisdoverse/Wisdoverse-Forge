import type { ReactNode } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'

const REPO_URL = 'https://github.com/Wisdoverse/wisdoverse-forge'
const APP_VERSION = typeof __APP_VERSION__ === 'string' ? __APP_VERSION__ : 'development'

export function AboutSection() {
  return (
    <div className="space-y-6" data-testid="settings-about">
      <div>
        <h2 className={uiStyles.sectionTitle}>About Wisdoverse Forge</h2>
        <p className={uiStyles.sectionDescription}>
          Check what you are using before asking for help or reporting an issue.
        </p>
      </div>

      <div>
        <h3 className={uiStyles.groupLabel}>Install details</h3>
        <dl className={cn(uiStyles.card, 'divide-y divide-[rgb(var(--border))]')}>
          <Row
            label="Product name"
            description="Use this name when sharing screenshots or asking an owner or admin for help."
            value="Wisdoverse Forge"
          />
          <Row
            label="Version"
            description="Share this number when something looks wrong after an update."
            value={APP_VERSION}
            valueTestId="settings-about-version"
          />
          <Row
            label="Project page"
            description="Open the public page for releases, issues, and contribution details."
            value={
              <a
                href={REPO_URL}
                target="_blank"
                rel="noreferrer"
                className="text-apple-blue hover:underline"
              >
                Open project page
              </a>
            }
          />
        </dl>
      </div>
    </div>
  )
}

function Row({
  label,
  description,
  value,
  valueTestId,
}: {
  label: string
  description: string
  value: ReactNode
  valueTestId?: string
}) {
  return (
    <div className="flex flex-col gap-2 px-4 py-3 sm:flex-row sm:items-start sm:justify-between">
      <dt>
        <span className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          {label}
        </span>
        <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {description}
        </p>
      </dt>
      <dd
        data-testid={valueTestId}
        className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark sm:text-right"
      >
        {value}
      </dd>
    </div>
  )
}
