import { useEffect, useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAdminStore, type AdminUser } from '@app/shared/model/admin.store'

const ROLE_OPTIONS = ['admin', 'operator', 'viewer'] as const
type Role = (typeof ROLE_OPTIONS)[number]

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
  const colors: Record<string, string> = {
    admin: 'bg-apple-blue/10 text-apple-blue',
    operator: 'bg-apple-blue/[0.07] text-apple-blue',
    viewer: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
  }
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full px-2 py-0.5 text-ui-caption font-medium',
        colors[role] ?? colors.viewer
      )}
    >
      {role}
    </span>
  )
}

function StatusBadge({ status }: { status: AdminUser['status'] }) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-ui-caption font-medium',
        status === 'active'
          ? 'bg-apple-blue/10 text-apple-blue'
          : 'bg-gray-100 text-gray-500 dark:bg-gray-800 dark:text-gray-400'
      )}
    >
      <span
        className={cn(
          'w-1.5 h-1.5 rounded-full',
          status === 'active' ? 'bg-apple-blue' : 'bg-gray-400'
        )}
      />
      {status}
    </span>
  )
}

function UserRow({ user }: { user: AdminUser }) {
  const [editing, setEditing] = useState(false)
  const [selectedRole, setSelectedRole] = useState<Role>(
    (ROLE_OPTIONS.includes(user.role as Role) ? user.role : 'viewer') as Role
  )
  const [saving, setSaving] = useState(false)
  const updateUserRole = useAdminStore((s) => s.updateUserRole)

  async function handleSave() {
    if (selectedRole === user.role) {
      setEditing(false)
      return
    }
    setSaving(true)
    const ok = await updateUserRole(user.id, selectedRole)
    setSaving(false)
    if (ok) setEditing(false)
  }

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
        {editing ? (
          <div className="flex items-center gap-2">
            <select
              value={selectedRole}
              onChange={(e) => setSelectedRole(e.target.value as Role)}
              className={cn(uiStyles.select, 'h-8 text-ui-caption')}
            >
              {ROLE_OPTIONS.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={() => void handleSave()}
              disabled={saving}
              className="text-ui-caption text-apple-blue hover:underline disabled:opacity-50"
            >
              {saving ? 'Saving...' : 'Save'}
            </button>
            <button
              type="button"
              onClick={() => setEditing(false)}
              className="text-ui-caption text-secondary-light hover:underline dark:text-secondary-dark"
            >
              Cancel
            </button>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className="group flex items-center gap-1.5"
            title="Click to edit role"
          >
            <RoleBadge role={user.role} />
            <span className="text-ui-caption text-secondary-light opacity-0 transition-opacity group-hover:opacity-100 dark:text-secondary-dark">
              edit
            </span>
          </button>
        )}
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
          'text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark'
        )}
      >
        {user.sessionsCount}
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

  function handleSearch(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault()
    void loadUsers(1)
  }

  const totalPages = Math.ceil(usersTotal / 25)

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Users</h2>
          <p className={uiStyles.sectionDescription}>{usersTotal} total users</p>
        </div>
      </div>

      {/* Search */}
      <form onSubmit={handleSearch} className="mb-4 flex gap-2">
        <input
          type="text"
          value={userSearch}
          onChange={(e) => setUserSearch(e.target.value)}
          placeholder="Search by email or name..."
          className={cn(uiStyles.input, 'min-w-0 flex-1')}
        />
        <button type="submit" className={uiStyles.primaryButton}>
          Search
        </button>
      </form>

      {/* Error */}
      {usersError && <div className={uiStyles.error}>{usersError}</div>}

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {usersLoading && users.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">Loading...</p>
          </div>
        ) : users.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No users found
            </p>
          </div>
        ) : (
          <table className={uiStyles.table}>
            <thead className={uiStyles.tableHead}>
              <tr>
                <th className={uiStyles.tableHeaderCell}>User</th>
                <th className={uiStyles.tableHeaderCell}>Role</th>
                <th className={uiStyles.tableHeaderCell}>Status</th>
                <th className={uiStyles.tableHeaderCell}>Created</th>
                <th className={uiStyles.tableHeaderCell}>Agents</th>
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
            Page {usersPage} of {totalPages}
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
