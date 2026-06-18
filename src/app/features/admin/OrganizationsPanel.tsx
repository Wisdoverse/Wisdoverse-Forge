import { useEffect } from 'react'
import { Building2, CalendarDays, Network, Users, type LucideIcon } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { type AdminOrg, useAdminStore } from '@app/shared/model/admin.store'
import { ADMIN_PANEL_RECOVERY, adminPanelLoadErrorMessage } from './adminErrorCopy'

function formatDate(iso: string): string {
  const date = new Date(iso)
  if (!Number.isFinite(date.getTime())) return 'Refresh team spaces to check created date'
  return date.toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}

function organizationReadiness(org: AdminOrg): {
  label: string
  tone: string
  nextStep: string
} {
  if (org.membersCount <= 0) {
    return {
      label: 'Needs people',
      tone: 'text-apple-red',
      nextStep: 'Invite at least one person so someone can use this team space.',
    }
  }
  if (org.teamsCount <= 0) {
    return {
      label: 'Needs a team',
      tone: 'text-secondary-light dark:text-secondary-dark',
      nextStep: 'Create a team so people have a place to organize projects.',
    }
  }
  return {
    label: 'Ready to use',
    tone: 'text-apple-blue',
    nextStep: 'People can create projects and start work from their teams.',
  }
}

const ORG_GUIDANCE: { title: string; description: string; Icon: LucideIcon }[] = [
  {
    title: 'Setup status shows what is missing',
    description: 'Use it to spot team spaces that still need their first people or team.',
    Icon: Building2,
  },
  {
    title: 'People show access size',
    description: 'A sudden jump can mean new people joined or access changed unexpectedly.',
    Icon: Users,
  },
  {
    title: 'Teams show work areas',
    description: 'More teams usually means more places to organize projects and assign agent work.',
    Icon: Network,
  },
]

function organizationSummary(orgs: AdminOrg[]): string {
  if (orgs.length === 0) {
    return 'Team spaces appear here after setup or sync. Create one before adding teams, projects, people, or places where new tasks wait.'
  }
  const members = orgs.reduce((total, org) => total + org.membersCount, 0)
  const teams = orgs.reduce((total, org) => total + org.teamsCount, 0)
  return `${members} people and ${teams} teams are spread across ${orgs.length} team space${
    orgs.length === 1 ? '' : 's'
  }.`
}

function OrganizationsGuide({ orgs }: { orgs: AdminOrg[] }) {
  return (
    <section
      data-testid="admin-org-guide"
      className="mb-4 rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2c2c2e]"
    >
      <div className="mb-3">
        <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          Admin view
        </p>
        <h3 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          Use team spaces to check setup at a glance
        </h3>
        <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
          {organizationSummary(orgs)}
        </p>
      </div>
      <div className="grid gap-2 md:grid-cols-3">
        {ORG_GUIDANCE.map(({ title, description, Icon }) => (
          <div key={title} className="rounded-lg bg-black/[0.03] p-3 dark:bg-white/[0.04]">
            <div className="mb-2 flex items-center gap-2 text-foreground-light dark:text-foreground-dark">
              <Icon size={14} strokeWidth={2.2} className="shrink-0 text-apple-blue" />
              <p className="text-ui-caption font-semibold">{title}</p>
            </div>
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {description}
            </p>
          </div>
        ))}
      </div>
    </section>
  )
}

function OrganizationsEmptyState() {
  return (
    <div
      data-testid="admin-org-empty"
      className="flex flex-col items-center justify-center px-4 py-12 text-center"
    >
      <div
        className="mb-3 flex h-10 w-10 items-center justify-center rounded-lg bg-black/[0.03] text-secondary-light dark:bg-white/[0.05] dark:text-secondary-dark"
        aria-hidden="true"
      >
        <Building2 size={18} strokeWidth={2} />
      </div>
      <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
        Create or sync a team space first
      </p>
      <p className="mt-1 max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
        Create or sync a team space before creating teams, projects, people, or places where new
        tasks wait. If you expected data here, confirm your admin access and refresh after Forge is
        ready.
      </p>
    </div>
  )
}

export function OrganizationsPanel() {
  const { orgs, orgsLoading, orgsError, loadOrgs } = useAdminStore()

  useEffect(() => {
    void loadOrgs()
  }, [loadOrgs])

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Team spaces</h2>
          <p className={uiStyles.sectionDescription}>
            Check whether each team space has the people and teams it needs to start work.
          </p>
        </div>
      </div>

      {/* Error */}
      {orgsError && (
        <div
          data-testid="admin-org-error"
          role="alert"
          aria-live="polite"
          className={uiStyles.error}
        >
          <p>{adminPanelLoadErrorMessage(orgsError, 'team space list')}</p>
          <p className="mt-1 text-ui-caption">{ADMIN_PANEL_RECOVERY}</p>
        </div>
      )}

      <OrganizationsGuide orgs={orgs} />

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {orgsLoading && orgs.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Loading team spaces…
            </p>
          </div>
        ) : orgs.length === 0 ? (
          <OrganizationsEmptyState />
        ) : (
          <table className={uiStyles.table}>
            <thead className={uiStyles.tableHead}>
              <tr>
                <th className={uiStyles.tableHeaderCell}>Team space</th>
                <th className={uiStyles.tableHeaderCell}>People</th>
                <th className={uiStyles.tableHeaderCell}>Teams</th>
                <th className={uiStyles.tableHeaderCell}>Setup status</th>
                <th className={uiStyles.tableHeaderCell}>Created</th>
                <th className={uiStyles.tableHeaderCell}>Admin hint</th>
              </tr>
            </thead>
            <tbody>
              {orgs.map((org) => {
                const readiness = organizationReadiness(org)
                return (
                  <tr
                    key={org.id}
                    className="border-b border-black/[0.06] transition-colors hover:bg-black/[0.02] dark:border-white/[0.08] dark:hover:bg-white/[0.02]"
                  >
                    <td className={uiStyles.tableCell}>
                      <div>
                        <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                          {org.name}
                        </p>
                        <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                          Automatic team space name: {org.slug}
                        </p>
                      </div>
                    </td>
                    <td
                      className={cn(
                        uiStyles.tableCell,
                        'text-ui-body tabular-nums text-foreground-light dark:text-foreground-dark'
                      )}
                    >
                      {org.membersCount}
                    </td>
                    <td
                      className={cn(
                        uiStyles.tableCell,
                        'text-ui-body tabular-nums text-foreground-light dark:text-foreground-dark'
                      )}
                    >
                      {org.teamsCount}
                    </td>
                    <td className={uiStyles.tableCell}>
                      <div className="max-w-[260px]">
                        <p className={cn('text-ui-body font-medium', readiness.tone)}>
                          {readiness.label}
                        </p>
                        <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                          {readiness.nextStep}
                        </p>
                      </div>
                    </td>
                    <td
                      className={cn(
                        uiStyles.tableCell,
                        'text-ui-caption text-secondary-light dark:text-secondary-dark'
                      )}
                    >
                      {formatDate(org.createdAt)}
                    </td>
                    <td className={uiStyles.tableCell}>
                      <span className="inline-flex items-center gap-1.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
                        <CalendarDays size={12} strokeWidth={2} aria-hidden="true" />
                        Review access when people or teams change.
                      </span>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
