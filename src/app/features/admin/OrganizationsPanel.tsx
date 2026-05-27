import { useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAdminStore, type AdminOrg } from '@app/shared/model/admin.store'

const PLAN_DETAILS: Record<string, { label: string; description: string }> = {
  free: {
    label: 'Free',
    description: 'Good for trying the product with a small team.',
  },
  pro: {
    label: 'Pro',
    description: 'Ready for regular team work.',
  },
  enterprise: {
    label: 'Enterprise',
    description: 'Configured for larger teams and stricter administration.',
  },
}

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

function planDescription(plan: string): string {
  return PLAN_DETAILS[plan]?.description ?? 'Plan details are not available yet.'
}

function organizationReadiness(org: AdminOrg): {
  label: string
  tone: string
  nextStep: string
} {
  if (org.membersCount <= 0) {
    return {
      label: 'Needs members',
      tone: 'text-apple-red',
      nextStep: 'Invite at least one member so someone can use this organization.',
    }
  }
  if (org.teamsCount <= 0) {
    return {
      label: 'Needs a team',
      tone: 'text-secondary-light dark:text-secondary-dark',
      nextStep: 'Create a team so members have a place to organize projects.',
    }
  }
  return {
    label: 'Ready to use',
    tone: 'text-apple-blue',
    nextStep: 'Members can create projects and start work from their teams.',
  }
}

function pluralize(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`
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
        <div role="alert" className={uiStyles.error}>
          Organizations could not be loaded. Check your admin access and try again. Detail:{' '}
          {orgsError}
        </div>
      )}

      {!orgsLoading && orgs.length > 0 && (
        <div className="mb-4 grid gap-1 rounded-card border border-black/[0.06] bg-black/[0.02] px-4 py-3 dark:border-white/[0.08] dark:bg-white/[0.03]">
          <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            {summary}
          </p>
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            Showing {pluralize(orgs.length, 'organization')} with{' '}
            {pluralize(totalMembers, 'member')} and {pluralize(totalTeams, 'team')}.
          </p>
        </div>
      )}

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {orgsLoading && orgs.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Loading organizations...
            </p>
          </div>
        ) : orgs.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-1 py-12 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No organizations found.
            </p>
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              Create or join an organization first, then it will appear here.
            </p>
          </div>
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
                          Organization URL name: {org.slug}
                        </p>
                      </div>
                    </td>
                    <td className={uiStyles.tableCell}>
                      <div className="grid gap-1">
                        <PlanBadge plan={org.plan} />
                        <p className="max-w-[220px] text-ui-caption text-secondary-light dark:text-secondary-dark">
                          {planDescription(org.plan)}
                        </p>
                      </div>
                    </td>
                    <td className={uiStyles.tableCell}>
                      <div>
                        <p className="text-ui-body tabular-nums text-foreground-light dark:text-foreground-dark">
                          {pluralize(org.membersCount, 'member')}
                        </p>
                        <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                          People with access
                        </p>
                      </div>
                    </td>
                    <td className={uiStyles.tableCell}>
                      <div>
                        <p className="text-ui-body tabular-nums text-foreground-light dark:text-foreground-dark">
                          {pluralize(org.teamsCount, 'team')}
                        </p>
                        <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                          Work areas
                        </p>
                      </div>
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
