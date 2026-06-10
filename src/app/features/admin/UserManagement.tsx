import { useEffect, type FormEvent } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAdminStore, type AdminUser } from '@app/shared/model/admin.store'

/**
 * Access levels the backend can report: `role` is derived from
 * `users.is_admin`, so it is always `admin` or `member`. The chip is
 * read-only — there is no role-editing endpoint.
 */
const ROLE_DETAILS: Record<Role, { label: string; description: string }> = {
  admin: {
    label: 'Admin',
    description: 'Can manage users, settings, and system configuration.',
  },
  member: {
    label: 'Member',
    description: 'Can run day-to-day work without changing admin settings.',
  },
}

type Role = 'admin' | 'member'

function normalizeRole(role: string): Role {
  return role === 'admin' ? 'admin' : 'member'
}

function formatDate(iso: string | null): string {
  if (!iso) return '—'
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

function RoleBadge({ role }: { role: string }) {
  const knownRole = normalizeRole(role)
  const colors: Record<Role, string> = {
    admin: 'bg-apple-blue/10 text-apple-blue',
    member: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
  }
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full px-2 py-0.5 text-ui-caption font-medium',
        colors[knownRole]
      )}
    >
      {ROLE_DETAILS[knownRole].label}
    </span>
  )
}

function StatusBadge({ status }: { status: AdminUser['status'] }) {
  const active = status === 'active'
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-ui-caption font-medium',
        active
          ? 'bg-apple-blue/10 text-apple-blue'
          : 'bg-gray-100 text-gray-500 dark:bg-gray-800 dark:text-gray-400'
      )}
    >
      <span className={cn('w-1.5 h-1.5 rounded-full', active ? 'bg-apple-blue' : 'bg-gray-400')} />
      {active ? 'Can sign in' : 'Access paused'}
    </span>
  )
}

function UserRow({ user }: { user: AdminUser }) {
  const role = normalizeRole(user.role)

  return (
    <tr className="border-b border-black/[0.06] transition-colors hover:bg-black/[0.02] dark:border-white/[0.08] dark:hover:bg-white/[0.02]">
      <td className={uiStyles.tableCell}>
        <div>
          <p className="max-w-[200px] truncate text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            {user.displayName}
          </p>
          <p className="max-w-[200px] truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
            {user.email}
          </p>
        </div>
      </td>
      <td className={uiStyles.tableCell}>
        <RoleBadge role={user.role} />
        <p className="mt-1 max-w-[220px] text-ui-caption text-secondary-light dark:text-secondary-dark">
          {ROLE_DETAILS[role].description}
        </p>
      </td>
      <td className={uiStyles.tableCell}>
        <StatusBadge status={user.status} />
      </td>
      <td
        className={cn(
          uiStyles.tableCell,
          'text-ui-caption text-secondary-light dark:text-secondary-dark'
        )}
      >
        {formatDate(user.createdAt)}
      </td>
      <td
        className={cn(
          uiStyles.tableCell,
          'text-ui-caption text-secondary-light dark:text-secondary-dark'
        )}
      >
        {formatDate(user.lastLoginAt)}
      </td>
    </tr>
  )
}

export function UserManagement() {
  const {
    users,
    usersTotal,
    usersPage,
    usersLoading,
    usersError,
    userSearch,
    loadUsers,
    setUserSearch,
  } = useAdminStore()

  useEffect(() => {
    void loadUsers(1)
  }, [loadUsers])

  function handleSearch(e: FormEvent<HTMLFormElement>) {
    e.preventDefault()
    void loadUsers(1)
  }

  const totalPages = Math.ceil(usersTotal / 25)

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>User access</h2>
          <p className={uiStyles.sectionDescription}>
            {usersTotal} people can be reviewed here. Access levels are read-only in this view.
          </p>
        </div>
      </div>

      {/* Search */}
      <form onSubmit={handleSearch} className="mb-4 flex gap-2">
        <input
          type="text"
          value={userSearch}
          onChange={(e) => setUserSearch(e.target.value)}
          placeholder="Search by name or email..."
          className={cn(uiStyles.input, 'min-w-0 flex-1')}
        />
        <button type="submit" className={uiStyles.primaryButton}>
          Find users
        </button>
      </form>

      {/* Error */}
      {usersError && <div className={uiStyles.error}>{usersError}</div>}

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {usersLoading && users.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Loading user access...
            </p>
          </div>
        ) : users.length === 0 ? (
          <div className="flex flex-col items-center justify-center px-6 py-12 text-center">
            <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              No users match this view
            </p>
            <p className="mt-1 max-w-sm text-ui-caption text-secondary-light dark:text-secondary-dark">
              Try a different name or email. New teammates appear here after they are invited.
            </p>
          </div>
        ) : (
          <table className={uiStyles.table}>
            <thead className={uiStyles.tableHead}>
              <tr>
                <th className={uiStyles.tableHeaderCell}>Person</th>
                <th className={uiStyles.tableHeaderCell}>Access level</th>
                <th className={uiStyles.tableHeaderCell}>Sign-in status</th>
                <th className={uiStyles.tableHeaderCell}>Added</th>
                <th className={uiStyles.tableHeaderCell}>Last sign-in</th>
              </tr>
            </thead>
            <tbody>
              {users.map((user) => (
                <UserRow key={user.id} user={user} />
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between mt-4">
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            Showing page {usersPage} of {totalPages}
          </p>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => void loadUsers(usersPage - 1)}
              disabled={usersPage <= 1 || usersLoading}
              className={uiStyles.secondaryButton}
            >
              Previous
            </button>
            <button
              type="button"
              onClick={() => void loadUsers(usersPage + 1)}
              disabled={usersPage >= totalPages || usersLoading}
              className={uiStyles.secondaryButton}
            >
              Next
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
