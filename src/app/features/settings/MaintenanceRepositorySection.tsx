import type { ReactNode } from 'react'
import { CheckCircle2, ExternalLink, RefreshCw, TriangleAlert } from 'lucide-react'
import type { SelfFixRepositorySetup } from '@shared/types/self-fix'
import { useAuth } from '@app/shared/model/auth.context'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import { useMaintenanceRepository } from './model/useMaintenanceRepository'
import { repositorySetupGuidance } from './model/repositorySetupGuidance'

const CONNECTION_GUIDE =
  'https://github.com/Wisdoverse/Wisdoverse-Forge/blob/main/docs/guides/self-fix-loop.md'
const LINK_STYLE =
  'text-apple-blue underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus'

export function MaintenanceRepositorySection() {
  const { user, isAuthenticated, isLoading } = useAuth()
  const enabled = !isLoading && isAuthenticated && user?.isAdmin === true
  const identity = `${user?.id ?? ''}:${user?.orgId ?? ''}`
  const { state, refresh } = useMaintenanceRepository(enabled, identity)
  const guidance = state.status === 'error' ? repositorySetupGuidance[state.reason] : null

  return (
    <section className="space-y-5" data-testid="settings-maintenance-repository">
      <div>
        <h2 className={uiStyles.sectionTitle}>Maintenance repository</h2>
        <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
          Check the one approved GitHub repository before preparing maintenance changes. A Forge
          administrator must connect its GitHub App first.
        </p>
      </div>
      {state.status === 'restricted' ? (
        <div className={uiStyles.note} role="status">
          Ask a Forge administrator to open this page and check the shared maintenance connection.
          Being a team owner does not grant this access.
        </div>
      ) : (
        <>
          <button
            type="button"
            onClick={refresh}
            disabled={state.status === 'loading'}
            className={uiStyles.secondaryButton}
          >
            <RefreshCw size={14} aria-hidden="true" />
            {state.status === 'loading' ? 'Checking connection…' : 'Check connection'}
          </button>
          {state.status === 'loading' && (
            <BeginnerLoadingState
              title="Checking repository connection"
              detail="Forge is checking the approved repository, starting version and access."
              nextStep="This check takes up to 30 seconds. If it cannot finish, use the recovery guidance and choose Check connection again."
              success="Success looks like the repository, its default branch and the access it allows."
              compact
              framed={false}
            />
          )}
          {guidance && (
            <div className={uiStyles.error} role="alert" aria-live="polite">
              <p className="font-medium">{guidance.title}</p>
              <p className="mt-1">{guidance.detail}</p>
            </div>
          )}
          {state.status === 'ready' && (
            <RepositorySnapshot snapshot={state.snapshot} checkedAt={state.checkedAt} />
          )}
        </>
      )}
      <div className="space-y-2 text-ui-body text-secondary-light dark:text-secondary-dark">
        <p>
          Before starting a task, confirm an agent is ready. Review each change and its automated
          checks before allowing a merge.
        </p>
        <div className="flex flex-wrap gap-x-4 gap-y-2">
          <a href="/tasks" className={LINK_STYLE}>
            Open the task board to submit maintenance work
          </a>
          <a href="/settings/runtime" className={LINK_STYLE}>
            Review agent setup
          </a>
          <a
            href={CONNECTION_GUIDE}
            target="_blank"
            rel="noopener noreferrer"
            className={LINK_STYLE}
          >
            Open connection guide <ExternalLink size={12} className="inline" aria-hidden="true" />
          </a>
        </div>
      </div>
    </section>
  )
}

function RepositorySnapshot({
  snapshot,
  checkedAt,
}: {
  snapshot: SelfFixRepositorySetup
  checkedAt: string
}) {
  const canPrepare = snapshot.contentsWrite && snapshot.pullRequestsWrite
  return (
    <div className="space-y-4" data-testid="maintenance-repository-snapshot">
      <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
        {canPrepare
          ? 'Repository connected for preparing changes'
          : 'Repository access needs attention'}
      </p>
      <dl className="divide-y divide-[rgb(var(--border))] border-y border-black/[0.06] dark:border-white/[0.08]">
        <SnapshotRow label="Approved repository">
          <a
            href={`https://github.com/${snapshot.repository}`}
            target="_blank"
            rel="noopener noreferrer"
            className={LINK_STYLE}
          >
            {snapshot.repository}
          </a>
        </SnapshotRow>
        <SnapshotRow label="Default branch">{snapshot.defaultBranch}</SnapshotRow>
        <SnapshotRow label="Starting version">
          <code className="break-all">{snapshot.baseSha}</code>
        </SnapshotRow>
        <SnapshotRow label="Last checked">
          <time dateTime={checkedAt}>{new Date(checkedAt).toLocaleString()}</time>
        </SnapshotRow>
      </dl>
      <PermissionList
        title="Preparing changes"
        items={[
          { label: 'Read and update project files', available: snapshot.contentsWrite },
          { label: 'Open change requests', available: snapshot.pullRequestsWrite },
        ]}
      />
      <PermissionList
        title="Reviewing and merging"
        items={[
          { label: 'Read automated check results', available: snapshot.checksRead },
          { label: 'Combine commits when merging', available: snapshot.squashMergeAllowed },
        ]}
      />
      <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        These are repository access settings. Each change still needs its own checks and human
        review. Check the connection again after changing repository settings.
      </p>
    </div>
  )
}

function SnapshotRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1 px-3 py-3 sm:grid-cols-[160px_minmax(0,1fr)] sm:gap-3">
      <dt className="text-ui-body text-secondary-light dark:text-secondary-dark">{label}</dt>
      <dd className="min-w-0 break-words text-ui-body text-foreground-light dark:text-foreground-dark">
        {children}
      </dd>
    </div>
  )
}

function PermissionList({
  title,
  items,
}: {
  title: string
  items: { label: string; available: boolean }[]
}) {
  return (
    <div>
      <h3 className={uiStyles.groupLabel}>{title}</h3>
      <ul className="mt-2 space-y-2">
        {items.map(({ label, available }) => (
          <li key={label} className="flex items-start gap-2 text-ui-body">
            {available ? (
              <CheckCircle2
                size={15}
                className="mt-0.5 shrink-0 text-apple-green"
                aria-hidden="true"
              />
            ) : (
              <TriangleAlert
                size={15}
                className="mt-0.5 shrink-0 text-apple-orange"
                aria-hidden="true"
              />
            )}
            <span className="flex min-w-0 flex-1 flex-wrap justify-between gap-x-3 gap-y-1 text-foreground-light dark:text-foreground-dark">
              <span>{label}</span>
              <span>{available ? 'Available' : 'Needs setup'}</span>
            </span>
          </li>
        ))}
      </ul>
      {items.some((item) => !item.available) && (
        <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Ask the repository owner to update the GitHub App access or repository merge settings,
          then choose Check connection.
        </p>
      )}
    </div>
  )
}
