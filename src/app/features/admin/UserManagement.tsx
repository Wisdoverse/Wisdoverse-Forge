import { useEffect, useState, type FormEvent } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAdminStore, type AdminUser } from '@app/shared/model/admin.store'

const ROLE_OPTIONS = ['admin', 'operator', 'viewer'] as const
type Role = (typeof ROLE_OPTIONS)[number]

const ROLE_LABELS: Record<Role, string> = {
  admin: 'Full access',
  operator: 'Can run work',
  viewer: 'View only',
}

const ROLE_HELP: Record<Role, string> = {
  admin: 'Can manage users, billing, settings, and work.',
  operator: 'Can run day-to-day agent and task work.',
  viewer: 'Can review information without changing it.',
}

const ROLE_DETAILS: Record<Role, { label: string; description: string }> = {
  admin: {
    label: 'Admin',
    description: 'Can manage users, settings, and system configuration.',
  },
  operator: {
    label: 'Operator',
    description: 'Can run daily workspace operations without changing admin settings.',
  },
  viewer: {
    label: 'Viewer',
    description: 'Can read workspace information without making changes.',
  },
}

function normalizeRole(role: string): Role {
  return ROLE_OPTIONS.includes(role as Role) ? (role as Role) : 'viewer'
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
  const knownRole = ROLE_OPTIONS.includes(role as Role) ? (role as Role) : 'viewer'
  const colors: Record<string, string> = {
    admin: 'bg-apple-blue/10 text-apple-blue',
    operator: 'bg-apple-blue/[0.07] text-apple-blue',
    viewer: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
  }
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full px-2 py-0.5 text-ui-caption font-medium',
        colors[knownRole]
      )}
    >
      {ROLE_LABELS[knownRole]}
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
  const [editing, setEditing] = useState(false)
  const [selectedRole, setSelectedRole] = useState<Role>(normalizeRole(user.role))
  const [saving, setSaving] = useState(false)
  const [roleError, setRoleError] = useState<string | null>(null)
  const updateUserRole = useAdminStore((s) => s.updateUserRole)
  const currentRole = normalizeRole(user.role)
  const selectedRoleDetails = ROLE_DETAILS[selectedRole]
  const roleChanged = selectedRole !== currentRole
  const editStatus = saving
    ? `Saving ${selectedRoleDetails.label} access...`
    : roleChanged
      ? `Ready to save ${selectedRoleDetails.label} access.`
      : 'Choose a different role before saving.'

  useEffect(() => {
    if (!editing) setSelectedRole(currentRole)
  }, [currentRole, editing])

  async function handleSave() {
    if (!roleChanged) {
      return
    }
    setSaving(true)
    setRoleError(null)
    const ok = await updateUserRole(user.id, selectedRole)
    setSaving(false)
    if (ok) {
      setEditing(false)
      return
    }
    setRoleError('Role could not be saved. Check your permissions and try again.')
  }

  function handleCancel() {
    setSelectedRole(currentRole)
    setRoleError(null)
    setEditing(false)
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
          <div className="grid min-w-[260px] gap-2">
            <select
              aria-label={`Role for ${user.displayName}`}
              value={selectedRole}
              onChange={(e) => {
                setSelectedRole(e.target.value as Role)
                setRoleError(null)
              }}
              className={cn(uiStyles.select, 'h-8 text-ui-caption')}
            >
              {ROLE_OPTIONS.map((r) => (
                <option key={r} value={r}>
                  {ROLE_LABELS[r]}
                </option>
              ))}
            </select>
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {selectedRoleDetails.description}
            </p>
            <p
              aria-live="polite"
              className="text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              {editStatus}
            </p>
            {roleError && (
              <p role="alert" className="text-ui-caption text-red-600 dark:text-red-400">
                {roleError}
              </p>
            )}
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={saving || !roleChanged}
                aria-label={`Save role for ${user.displayName}`}
                className={cn(uiStyles.primaryButton, 'h-8 px-3 text-ui-caption')}
              >
                {saving ? 'Saving...' : 'Save Role'}
              </button>
              <button
                type="button"
                onClick={handleCancel}
                className={cn(uiStyles.secondaryButton, 'h-8 px-3 text-ui-caption')}
              >
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className="group flex items-center gap-1.5"
            title="Change what this user can do"
          >
            <RoleBadge role={user.role} />
            <span className="text-ui-caption text-secondary-light opacity-0 transition-opacity group-hover:opacity-100 dark:text-secondary-dark">
              change access
            </span>
          </button>
        )}
        {!editing && (
          <p className="mt-1 max-w-[220px] text-ui-caption text-secondary-light dark:text-secondary-dark">
            {ROLE_HELP[ROLE_OPTIONS.includes(user.role as Role) ? (user.role as Role) : 'viewer']}
          </p>
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
        {user.sessionsCount === 0 ? 'No active sessions' : `${user.sessionsCount} active`}
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
            {usersTotal} people can be reviewed here. Change access only when their job changes.
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
                <th className={uiStyles.tableHeaderCell}>Active sessions</th>
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
