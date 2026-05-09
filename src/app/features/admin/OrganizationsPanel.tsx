import { useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAdminStore } from '@app/shared/model/admin.store'

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
      {plan}
    </span>
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
          <h2 className={uiStyles.sectionTitle}>Organizations</h2>
          <p className={uiStyles.sectionDescription}>{orgs.length} total organizations</p>
        </div>
      </div>

      {/* Error */}
      {orgsError && <div className={uiStyles.error}>{orgsError}</div>}

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {orgsLoading && orgs.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">Loading...</p>
          </div>
        ) : orgs.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No organizations found
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
                <th className={uiStyles.tableHeaderCell}>Created</th>
              </tr>
            </thead>
            <tbody>
              {orgs.map((org) => (
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
                        {org.slug}
                      </p>
                    </div>
                  </td>
                  <td className={uiStyles.tableCell}>
                    <PlanBadge plan={org.plan} />
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
                  <td
                    className={cn(
                      uiStyles.tableCell,
                      'text-ui-caption text-secondary-light dark:text-secondary-dark'
                    )}
                  >
                    {formatDate(org.createdAt)}
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
