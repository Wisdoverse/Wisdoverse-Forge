import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { RefreshCw } from 'lucide-react'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { isNewerVersion } from '@app/shared/lib/versionCheck'

const REPO_URL = 'https://github.com/Wisdoverse/wisdoverse-forge'
const RELEASES_API_URL = 'https://api.github.com/repos/Wisdoverse/Wisdoverse-Forge/releases/latest'
const APP_VERSION = typeof __APP_VERSION__ === 'string' ? __APP_VERSION__ : 'development'

type UpdateCheckState =
  | { status: 'idle' }
  | { status: 'checking' }
  | { status: 'up-to-date'; latest: string }
  | { status: 'update-available'; latest: string }
  | { status: 'error' }

function UpdateCheckButton({ version }: { version: string }) {
  const { t } = useTranslation()
  const [state, setState] = useState<UpdateCheckState>({ status: 'idle' })

  async function check() {
    setState({ status: 'checking' })
    try {
      const res = await fetch(RELEASES_API_URL, {
        headers: { Accept: 'application/vnd.github+json' },
      })
      if (!res.ok) {
        setState({ status: 'error' })
        return
      }
      const data = (await res.json()) as { tag_name?: string }
      const latest = data.tag_name
      if (!latest) {
        setState({ status: 'error' })
        return
      }
      setState(
        isNewerVersion(latest, version)
          ? { status: 'update-available', latest }
          : { status: 'up-to-date', latest }
      )
    } catch {
      setState({ status: 'error' })
    }
  }

  if (state.status === 'update-available') {
    return (
      <a
        href={REPO_URL}
        target="_blank"
        rel="noreferrer"
        data-testid="settings-about-update-available"
        className="font-medium text-apple-blue underline-offset-2 hover:underline"
      >
        {t('about.update.available', { version: state.latest })}
      </a>
    )
  }
  if (state.status === 'up-to-date') {
    return (
      <span data-testid="settings-about-up-to-date" className="font-medium text-apple-green">
        {t('about.update.upToDate', { version: state.latest })}
      </span>
    )
  }
  if (state.status === 'error') {
    return (
      <button
        type="button"
        data-testid="settings-about-update-retry"
        onClick={() => void check()}
        className="font-medium text-apple-orange underline-offset-2 hover:underline"
      >
        {t('about.update.unreachable')}
      </button>
    )
  }
  return (
    <button
      type="button"
      data-testid="settings-about-update-check"
      onClick={() => void check()}
      disabled={state.status === 'checking'}
      className="inline-flex items-center gap-1.5 font-medium text-apple-blue underline-offset-2 hover:underline disabled:cursor-wait disabled:opacity-60"
    >
      {state.status === 'checking' ? (
        <RefreshCw size={13} strokeWidth={2} className="animate-spin" aria-hidden="true" />
      ) : null}
      {state.status === 'checking' ? t('about.update.checking') : t('about.update.check')}
    </button>
  )
}

export function AboutSection() {
  const { t } = useTranslation()
  return (
    <div className="space-y-6" data-testid="settings-about">
      <div>
        <h2 className={uiStyles.sectionTitle}>About Wisdoverse Forge</h2>
      </div>

      <div>
        <h3 className={uiStyles.groupLabel}>App details</h3>
        <dl className="divide-y divide-[rgb(var(--border))] border-y border-black/[0.06] bg-transparent dark:border-white/[0.08]">
          <Row
            label="Version"
            description="Share this number when something looks wrong after an update."
            value={APP_VERSION}
            valueTestId="settings-about-version"
          />
          <Row
            label="Project page"
            description="Open the public page for updates, fixes, and project details."
            value={
              <a
                href={REPO_URL}
                target="_blank"
                rel="noreferrer"
                className="underline-offset-2 hover:underline"
              >
                Open project page
              </a>
            }
          />
          <Row
            label={t('about.update.label')}
            description={t('about.update.description')}
            value={<UpdateCheckButton version={APP_VERSION} />}
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
