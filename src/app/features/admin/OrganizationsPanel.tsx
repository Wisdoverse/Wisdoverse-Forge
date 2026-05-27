import { useEffect } from 'react'
import { Building2, CalendarDays, Network, Users, type LucideIcon } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { type AdminOrg, useAdminStore } from '@app/shared/model/admin.store'

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    })
  } catch {
    return '—'
  }
}

function PlanBadge({ plan }: { plan: string }) {
  const details = PLAN_DETAILS[plan] ?? {
    label: plan || 'Unknown',
    description: 'Plan details are not available yet.',
  }
  const colors: Record<string, string> = {
    free: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
    pro: 'bg-apple-blue/10 text-apple-blue',
    enterprise: 'bg-apple-blue/[0.07] text-apple-blue',
  }
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full px-2 py-0.5 text-ui-caption font-medium',
        colors[plan] ?? colors.free
      )}
    >
      {details.label}
    </span>
  )
}

const ORG_GUIDANCE: { title: string; description: string; Icon: LucideIcon }[] = [
  {
    title: 'Plan shows limits',
    description: 'Use it to spot which organizations may need billing or capacity review.',
    Icon: Building2,
  },
  {
    title: 'Members show access size',
    description: 'A sudden jump can mean onboarding succeeded or access needs review.',
    Icon: Users,
  },
  {
    title: 'Teams show routing shape',
    description: 'More teams usually means more project lanes and agent assignment paths.',
    Icon: Network,
  },
]

function organizationSummary(orgs: AdminOrg[]): string {
  if (orgs.length === 0) {
    return 'Organizations appear here after setup or sync. Teams, projects, and members need an organization first.'
  }
  const members = orgs.reduce((total, org) => total + org.membersCount, 0)
  const teams = orgs.reduce((total, org) => total + org.teamsCount, 0)
  return `${members} members and ${teams} teams are spread across ${orgs.length} organization${
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
          Use organizations to check tenant setup at a glance
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
        No organizations are visible yet
      </p>
      <p className="mt-1 max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
        Create or sync an organization before creating teams, projects, members, or agent work
        lanes. If you expected data here, confirm your admin access and refresh after the API is
        healthy.
      </p>
    </div>
  )
}

export function OrganizationsPanel() {
  const { orgs, orgsLoading, orgsError, loadOrgs } = useAdminStore()

  useEffect(() => {
    void loadOrgs()
  }, [loadOrgs])

  const totalMembers = orgs.reduce((total, org) => total + org.membersCount, 0)
  const totalTeams = orgs.reduce((total, org) => total + org.teamsCount, 0)
  const organizationsNeedingSetup = orgs.filter(
    (org) => org.membersCount <= 0 || org.teamsCount <= 0
  ).length
  const summary =
    orgs.length === 0
      ? 'No organizations are visible yet.'
      : organizationsNeedingSetup === 0
        ? 'All organizations have members and teams.'
        : `${organizationsNeedingSetup} ${organizationsNeedingSetup === 1 ? 'organization needs' : 'organizations need'} setup before teams can use them.`

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Organizations</h2>
          <p className={uiStyles.sectionDescription}>
            Check whether each organization has people, teams, and a plan that matches its use.
          </p>
        </div>
      </div>

      {/* Error */}
      {orgsError && (
        <div data-testid="admin-org-error" role="alert" className={uiStyles.error}>
          <p>{orgsError}</p>
          <p className="mt-1 text-ui-caption">
            Refresh after the API is healthy, or confirm this account has admin access.
          </p>
        </div>
      )}

      <OrganizationsGuide orgs={orgs} />

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {orgsLoading && orgs.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Loading organizations…
            </p>
          </div>
        ) : orgs.length === 0 ? (
          <OrganizationsEmptyState />
        ) : (
          <table className={uiStyles.table}>
            <thead className={uiStyles.tableHead}>
              <tr>
                <th className={uiStyles.tableHeaderCell}>Name</th>
                <th className={uiStyles.tableHeaderCell}>Plan</th>
                <th className={uiStyles.tableHeaderCell}>Members</th>
                <th className={uiStyles.tableHeaderCell}>Teams</th>
                <th className={uiStyles.tableHeaderCell}>Readiness</th>
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
                      Review access when membership or teams change.
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
