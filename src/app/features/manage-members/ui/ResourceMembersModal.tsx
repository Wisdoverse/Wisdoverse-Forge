import { useEffect, useMemo, useState } from 'react'
import {
  AlertTriangle,
  Info,
  Search,
  ShieldCheck,
  Trash2,
  UserPlus,
  Users,
  X,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type {
  AddResourceMemberInput,
  ResourceMember,
  ResourceMemberRole,
  UpdateResourceMemberInput,
} from '@app/entities/member'
import type { OrgUser } from '@app/entities/user'
import { resourceMemberErrorMessage } from '../model/resourceMemberErrorMessages'

const ROLE_OPTIONS: Array<{ value: ResourceMemberRole; label: string }> = [
  { value: 'owner', label: 'Owner' },
  { value: 'admin', label: 'Admin' },
  { value: 'maintainer', label: 'Maintainer' },
  { value: 'member', label: 'Member' },
]

const ROLE_TONE: Record<ResourceMemberRole, string> = {
  owner: 'border-apple-blue/30 bg-apple-blue/10 text-apple-blue',
  admin: 'border-apple-blue/20 bg-apple-blue/10 text-apple-blue',
  maintainer: 'border-apple-blue/15 bg-apple-blue/[0.07] text-apple-blue',
  member:
    'border-black/10 bg-black/[0.03] text-secondary-light dark:border-white/10 dark:bg-white/[0.05] dark:text-secondary-dark',
}

const MEMBER_ROLE_GUIDANCE: {
  title: string
  description: string
  Icon: LucideIcon
}[] = [
  {
    title: 'Start with Member access',
    description: 'Use this for people who only need normal access to this team or project.',
    Icon: Users,
  },
  {
    title: 'Use Maintainer access for everyday changes',
    description: 'Maintainers can help manage day-to-day work without deciding who gets access.',
    Icon: ShieldCheck,
  },
  {
    title: 'Keep Owner and Admin access limited',
    description: 'Choose these only for people who should manage access for everyone else.',
    Icon: Info,
  },
]

interface ResourceMembersModalProps {
  resourceLabel: 'Team' | 'Project'
  resourceName: string
  loadMembers: () => Promise<ResourceMember[]>
  loadUsers: () => Promise<OrgUser[]>
  addMember: (input: AddResourceMemberInput) => Promise<ResourceMember>
  updateMember: (userId: string, input: UpdateResourceMemberInput) => Promise<ResourceMember>
  removeMember: (userId: string) => Promise<void>
  onClose: () => void
}

export function ResourceMembersModal({
  resourceLabel,
  resourceName,
  loadMembers,
  loadUsers,
  addMember,
  updateMember,
  removeMember,
  onClose,
}: ResourceMembersModalProps) {
  const [members, setMembers] = useState<ResourceMember[]>([])
  const [users, setUsers] = useState<OrgUser[]>([])
  const [selectedUserId, setSelectedUserId] = useState('')
  const [selectedRole, setSelectedRole] = useState<ResourceMemberRole>('member')
  const [memberFilter, setMemberFilter] = useState('')
  const [loading, setLoading] = useState(true)
  const [busyKey, setBusyKey] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [confirmRemoveUserId, setConfirmRemoveUserId] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)

    Promise.all([loadMembers(), loadUsers()])
      .then(([loadedMembers, loadedUsers]) => {
        if (cancelled) return
        setMembers(loadedMembers)
        setUsers(loadedUsers)
      })
      .catch((err) => {
        if (cancelled) return
        setError(resourceMemberErrorMessage('load', resourceLabel, err))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [loadMembers, loadUsers, resourceLabel])

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [onClose])

  const candidateUsers = useMemo(() => {
    const memberIds = new Set(members.map((member) => member.userId))
    return users.filter((user) => !memberIds.has(user.id))
  }, [members, users])

  const filteredCandidateUsers = useMemo(() => {
    const query = memberFilter.trim().toLowerCase()
    if (!query) return candidateUsers
    return candidateUsers.filter((user) => {
      const haystack = `${user.username} ${user.email}`.toLowerCase()
      return haystack.includes(query)
    })
  }, [candidateUsers, memberFilter])

  useEffect(() => {
    if (selectedUserId && !filteredCandidateUsers.some((user) => user.id === selectedUserId)) {
      setSelectedUserId('')
    }
  }, [filteredCandidateUsers, selectedUserId])

  const candidateStatus = useMemo(
    () =>
      describeCandidateStatus({
        loading,
        query: memberFilter,
        users,
        candidateUsers,
        filteredCandidateUsers,
      }),
    [candidateUsers, filteredCandidateUsers, loading, memberFilter, users]
  )

  async function handleAddMember() {
    if (!selectedUserId) return
    setBusyKey('add')
    setError(null)
    try {
      const member = await addMember({ userId: selectedUserId, role: selectedRole })
      setMembers((prev) => [...prev.filter((item) => item.userId !== member.userId), member])
      setSelectedUserId('')
      setSelectedRole('member')
      setConfirmRemoveUserId(null)
    } catch (err) {
      setError(resourceMemberErrorMessage('add', resourceLabel, err))
    } finally {
      setBusyKey(null)
    }
  }

  async function handleRoleChange(member: ResourceMember, role: ResourceMemberRole) {
    if (member.role === role) return
    const key = `role:${member.userId}`
    setBusyKey(key)
    setError(null)
    try {
      const updated = await updateMember(member.userId, { role })
      setMembers((prev) => prev.map((item) => (item.userId === member.userId ? updated : item)))
    } catch (err) {
      setError(resourceMemberErrorMessage('updateRole', resourceLabel, err))
    } finally {
      setBusyKey(null)
    }
  }

  async function handleRemoveMember(member: ResourceMember) {
    const key = `remove:${member.userId}`
    setBusyKey(key)
    setError(null)
    try {
      await removeMember(member.userId)
      setMembers((prev) => prev.filter((item) => item.userId !== member.userId))
      setConfirmRemoveUserId(null)
    } catch (err) {
      setError(resourceMemberErrorMessage('remove', resourceLabel, err))
    } finally {
      setBusyKey(null)
    }
  }

  const addStatusId = 'resource-members-add-status'
  const roleHelpId = 'resource-members-role-help'

  return (
    <div
      className="fixed inset-0 z-50 flex items-end justify-center bg-black/45 p-0 backdrop-blur-sm sm:items-center sm:p-4"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="resource-members-title"
        className={cn(
          'max-h-[min(720px,calc(100vh-32px))] w-full overflow-hidden rounded-t-card border sm:max-w-2xl sm:rounded-card',
          'border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2c2c2e]'
        )}
      >
        <div className="flex items-center justify-between border-b border-black/[0.06] px-4 py-4 dark:border-white/[0.08] sm:px-5">
          <div className="flex min-w-0 items-center gap-3">
            <div
              className={cn(
                'flex h-10 w-10 shrink-0 items-center justify-center rounded-lg',
                'bg-apple-blue/10 text-apple-blue ring-1 ring-apple-blue/15'
              )}
              aria-hidden="true"
            >
              <Users size={18} strokeWidth={2} />
            </div>
            <div className="min-w-0">
              <div className="flex min-w-0 items-center gap-2">
                <h2
                  id="resource-members-title"
                  className="truncate text-ui-title font-semibold text-foreground-light dark:text-foreground-dark"
                >
                  {resourceLabel} members
                </h2>
                <span className={cn(uiStyles.badge, 'shrink-0 tabular-nums')}>
                  {members.length}
                </span>
              </div>
              <p className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                {resourceName}
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close members dialog"
            title="Close"
            className="flex h-8 w-8 shrink-0 touch-manipulation items-center justify-center rounded-lg text-secondary-light transition-colors hover:bg-black/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/40 dark:text-secondary-dark dark:hover:bg-white/5"
          >
            <X size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        </div>

        <div className="max-h-[calc(100vh-120px)] space-y-4 overflow-y-auto overscroll-contain p-4 sm:p-5">
          {error && (
            <div role="alert" aria-live="polite" className={uiStyles.error}>
              {error}
            </div>
          )}

          <MemberRoleGuide resourceLabel={resourceLabel} />

          <div className="rounded-card border border-black/[0.08] bg-black/[0.015] p-3 dark:border-white/[0.08] dark:bg-white/[0.025]">
            <div className="mb-3">
              <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                Add people already in your team space
              </p>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Search for a person, choose what they can do, then add them to this{' '}
                {resourceLabel.toLowerCase()}. You can change this later.
              </p>
            </div>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)_auto_auto]">
              <div className="relative">
                <Search
                  className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
                  size={14}
                  strokeWidth={2}
                  aria-hidden="true"
                />
                <input
                  type="search"
                  name="memberFilter"
                  value={memberFilter}
                  onChange={(event) => setMemberFilter(event.target.value)}
                  disabled={loading || candidateUsers.length === 0}
                  aria-label="Filter team-space people"
                  aria-describedby={addStatusId}
                  autoComplete="off"
                  placeholder="Search people by name or email…"
                  className={cn(uiStyles.input, 'min-w-0 pl-8')}
                />
              </div>
              <select
                name="memberUserId"
                value={selectedUserId}
                onChange={(event) => setSelectedUserId(event.target.value)}
                disabled={loading || busyKey === 'add' || filteredCandidateUsers.length === 0}
                aria-label="Select person to add"
                aria-describedby={addStatusId}
                className={cn(uiStyles.select, 'min-w-0')}
              >
                <option value="">{candidateStatus.selectLabel}</option>
                {filteredCandidateUsers.map((user) => (
                  <option key={user.id} value={user.id}>
                    {user.username} ({user.email})
                  </option>
                ))}
              </select>
              <select
                name="memberRole"
                value={selectedRole}
                onChange={(event) => setSelectedRole(event.target.value as ResourceMemberRole)}
                disabled={loading || busyKey === 'add'}
                aria-label="New member access level"
                aria-describedby={roleHelpId}
                className={uiStyles.select}
              >
                {ROLE_OPTIONS.map((role) => (
                  <option key={role.value} value={role.value}>
                    {role.label}
                  </option>
                ))}
              </select>
              <button
                type="button"
                onClick={() => void handleAddMember()}
                disabled={!selectedUserId || loading || busyKey === 'add'}
                aria-busy={busyKey === 'add'}
                aria-describedby={addStatusId}
                className={uiStyles.primaryButton}
              >
                <UserPlus size={14} strokeWidth={2} aria-hidden="true" />
                <span>{busyKey === 'add' ? 'Adding…' : 'Add'}</span>
              </button>
            </div>
            <p
              data-testid="member-candidate-status"
              className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              {candidateStatus.detail}
            </p>
          </div>

          <div className="overflow-hidden rounded-card border border-black/[0.08] bg-white dark:border-white/[0.08] dark:bg-black/10">
            <div className="flex items-center justify-between border-b border-black/[0.06] bg-black/[0.015] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.025]">
              <div className="flex min-w-0 items-center gap-2">
                <ShieldCheck
                  size={14}
                  strokeWidth={2}
                  className="text-secondary-light dark:text-secondary-dark"
                  aria-hidden="true"
                />
                <span className="truncate text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
                  People with access
                </span>
              </div>
              <span className="text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
                {members.length} members
              </span>
            </div>
            <div className="max-h-[360px] overflow-y-auto overscroll-contain">
              {loading ? (
                <div
                  role="status"
                  className="px-4 py-10 text-center text-ui-body text-secondary-light dark:text-secondary-dark"
                >
                  Loading members…
                </div>
              ) : members.length === 0 ? (
                <div
                  data-testid="members-empty-state"
                  className="flex flex-col items-center gap-2 px-4 py-10 text-center text-ui-body text-secondary-light dark:text-secondary-dark"
                >
                  <div
                    className="flex h-9 w-9 items-center justify-center rounded-lg bg-black/[0.03] text-secondary-light dark:bg-white/[0.05] dark:text-secondary-dark"
                    aria-hidden="true"
                  >
                    <Users size={17} strokeWidth={2} />
                  </div>
                  <span className="font-medium text-foreground-light dark:text-foreground-dark">
                    Add the first direct member
                  </span>
                  <p className="max-w-md text-ui-caption">
                    Add a person from the team space above to give them access to this{' '}
                    {resourceLabel.toLowerCase()}. Start with Member access unless they need to
                    manage who can get in.
                  </p>
                </div>
              ) : (
                members.map((member) => (
                  <div
                    key={member.userId}
                    className="flex items-center justify-between gap-3 border-b border-black/[0.06] px-3 py-3 transition-colors last:border-b-0 hover:bg-black/[0.015] dark:border-white/[0.08] dark:hover:bg-white/[0.025]"
                  >
                    <div className="flex min-w-0 items-center gap-3">
                      <div
                        className={cn(
                          'flex h-9 w-9 shrink-0 items-center justify-center rounded-lg',
                          'bg-black/[0.04] text-ui-caption font-semibold uppercase text-foreground-light',
                          'ring-1 ring-black/5 dark:bg-white/[0.06] dark:text-foreground-dark dark:ring-white/10'
                        )}
                        aria-hidden="true"
                      >
                        {initialsFor(member)}
                      </div>
                      <div className="min-w-0">
                        <p className="truncate text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                          {member.username || member.email}
                        </p>
                        <p className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                          {member.email}
                        </p>
                      </div>
                    </div>
                    <div className="flex shrink-0 items-center gap-1.5">
                      <span
                        className={cn(
                          'hidden rounded-badge border px-2 py-1 text-[10px] font-medium sm:inline-flex',
                          ROLE_TONE[roleValue(member.role)]
                        )}
                      >
                        {roleLabel(member.role)}
                      </span>
                      <select
                        value={roleValue(member.role)}
                        onChange={(event) =>
                          void handleRoleChange(member, event.target.value as ResourceMemberRole)
                        }
                        disabled={busyKey !== null}
                        aria-label={`Access level for ${member.username || member.email}`}
                        className={cn(uiStyles.select, 'h-8 text-ui-caption')}
                      >
                        {ROLE_OPTIONS.map((role) => (
                          <option key={role.value} value={role.value}>
                            {role.label}
                          </option>
                        ))}
                      </select>
                      {confirmRemoveUserId === member.userId ? (
                        <div className="flex max-w-full flex-col items-end gap-2 sm:max-w-xs">
                          <div className="flex items-start gap-1.5 rounded-lg border border-apple-red/20 bg-apple-red/10 px-2 py-1.5 text-left text-ui-caption text-apple-red">
                            <AlertTriangle
                              size={14}
                              strokeWidth={2}
                              className="mt-0.5 shrink-0"
                              aria-hidden="true"
                            />
                            <span>
                              Removing access stops {member.username || member.email} from opening
                              this {resourceLabel.toLowerCase()}.
                            </span>
                          </div>
                          <div className="flex items-center gap-1.5">
                            <button
                              type="button"
                              onClick={() => void handleRemoveMember(member)}
                              disabled={busyKey !== null}
                              aria-label={`Remove access for ${member.username || member.email}`}
                              className="inline-flex h-8 items-center justify-center rounded-lg bg-apple-red px-2 text-ui-caption font-medium text-white transition-colors hover:bg-apple-red/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-500/30 disabled:cursor-not-allowed disabled:opacity-50"
                            >
                              {busyKey === `remove:${member.userId}`
                                ? 'Removing...'
                                : 'Remove access'}
                            </button>
                            <button
                              type="button"
                              onClick={() => setConfirmRemoveUserId(null)}
                              disabled={busyKey !== null}
                              aria-label={`Keep access for ${member.username || member.email}`}
                              className="inline-flex h-8 items-center justify-center rounded-lg px-2 text-ui-caption font-medium text-secondary-light transition-colors hover:bg-black/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/40 disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/5"
                            >
                              Keep access
                            </button>
                          </div>
                        </div>
                      ) : (
                        <button
                          type="button"
                          onClick={() => setConfirmRemoveUserId(member.userId)}
                          disabled={busyKey !== null}
                          aria-label={`Remove ${member.username || member.email}`}
                          title="Remove"
                          className="flex h-8 w-8 touch-manipulation items-center justify-center rounded-lg text-secondary-light transition-colors hover:bg-red-50 hover:text-red-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-500/30 disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-red-900/20 dark:hover:text-red-400"
                        >
                          <Trash2 size={14} strokeWidth={2} aria-hidden="true" />
                        </button>
                      )}
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function MemberRoleGuide({ resourceLabel }: { resourceLabel: 'Team' | 'Project' }) {
  return (
    <section
      data-testid="member-role-guide"
      className="rounded-card border border-black/[0.08] bg-white p-3 dark:border-white/[0.08] dark:bg-black/10"
    >
      <div className="mb-3">
        <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          Who should get access
        </p>
        <h3 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          Add people only when they need this {resourceLabel.toLowerCase()}
        </h3>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Only people in this team space can be added here. Invite the person to the team space
          first if they do not appear in the list.
        </p>
      </div>
      <div className="grid gap-2 sm:grid-cols-3">
        {MEMBER_ROLE_GUIDANCE.map(({ title, description, Icon }) => (
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

function describeCandidateStatus({
  loading,
  query,
  users,
  candidateUsers,
  filteredCandidateUsers,
}: {
  loading: boolean
  query: string
  users: OrgUser[]
  candidateUsers: OrgUser[]
  filteredCandidateUsers: OrgUser[]
}): { selectLabel: string; detail: string } {
  if (loading) {
    return {
      selectLabel: 'Loading team-space people',
      detail: 'Loading people in this team space and the people already listed below.',
    }
  }
  if (users.length === 0) {
    return {
      selectLabel: 'Invite someone to the team space first',
      detail: 'Invite the person to the team space first, then return here to give access.',
    }
  }
  if (candidateUsers.length === 0) {
    return {
      selectLabel: 'Everyone already has access',
      detail: 'Every team-space person is already listed below for this team or project.',
    }
  }
  if (query.trim() && filteredCandidateUsers.length === 0) {
    return {
      selectLabel: 'Clear search or invite this person first',
      detail: 'Clear the filter or invite the person to the team space before adding them here.',
    }
  }
  return {
    selectLabel: 'Select person to add',
    detail: 'Choose a person, pick the safest access level, then add them here.',
  }
}

function roleValue(role: string): ResourceMemberRole {
  if (role === 'owner') return 'owner'
  if (role === 'admin') return 'admin'
  if (role === 'maintainer') return 'maintainer'
  return 'member'
}

function roleLabel(role: string): string {
  const normalized = roleValue(role)
  return ROLE_OPTIONS.find((option) => option.value === normalized)?.label ?? 'Member'
}

function initialsFor(member: ResourceMember): string {
  const source = (member.username || member.email || 'U').trim()
  const parts = source.split(/[\s@._-]+/).filter(Boolean)
  const initials = parts
    .slice(0, 2)
    .map((part) => part[0])
    .join('')
  return (initials || 'U').toUpperCase()
}
