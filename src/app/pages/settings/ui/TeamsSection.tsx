import { useCallback, useEffect, useState } from 'react'
import { Plus } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAuth } from '@app/shared/model/auth.context'
import { ResourceMembersModal } from '@app/features/manage-members'
import { CreateTeamForm, EditableTeamRow } from '@app/features/manage-team'
import { userApi } from '@app/entities/user'
import { teamApi, type NavTeam, type UpdateTeamInput } from '@app/entities/team'
import { workspaceSettingsErrorMessage } from '../model/workspaceSettingsErrorMessage'

export function TeamsSection() {
  const { user } = useAuth()
  const canCreateTeam = user?.role === 'owner' || user?.role === 'admin'
  const [teams, setTeams] = useState<NavTeam[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [showForm, setShowForm] = useState(false)
  const [saving, setSaving] = useState(false)
  const [membersTeam, setMembersTeam] = useState<NavTeam | null>(null)
  const loadOrgUsers = useCallback(() => userApi.getUsers(), [])

  const loadTeams = useCallback(async () => {
    const orgId = user?.orgId
    if (!orgId) return
    setLoading(true)
    setError(null)
    try {
      const result = await teamApi.getTeams(orgId)
      setTeams(result)
    } catch (err) {
      setError(workspaceSettingsErrorMessage('team', 'load', err))
    } finally {
      setLoading(false)
    }
  }, [user?.orgId])

  useEffect(() => {
    void loadTeams()
  }, [loadTeams])

  async function handleCreate(name: string) {
    const orgId = user?.orgId
    if (!orgId) return
    setSaving(true)
    setError(null)
    try {
      const team = await teamApi.createTeam(orgId, { name })
      setTeams((prev) => [...prev, team])
      setShowForm(false)
    } catch (err) {
      setError(workspaceSettingsErrorMessage('team', 'create', err))
    } finally {
      setSaving(false)
    }
  }

  async function handleUpdate(teamId: string, input: UpdateTeamInput) {
    const orgId = user?.orgId
    if (!orgId) return
    const updated = await teamApi.updateTeam(orgId, teamId, input)
    setTeams((prev) => prev.map((team) => (team.id === teamId ? { ...team, ...updated } : team)))
  }

  async function handleDelete(teamId: string) {
    const orgId = user?.orgId
    if (!orgId) return
    await teamApi.deleteTeam(orgId, teamId)
    setTeams((prev) => prev.filter((team) => team.id !== teamId))
    setMembersTeam((current) => (current?.id === teamId ? null : current))
  }

  const loadSelectedTeamMembers = useCallback(async () => {
    const orgId = user?.orgId
    if (!orgId || !membersTeam) return []
    return teamApi.getMembers(orgId, membersTeam.id)
  }, [membersTeam, user?.orgId])

  const addSelectedTeamMember = useCallback(
    async (input: Parameters<typeof teamApi.addMember>[2]) => {
      const orgId = user?.orgId
      if (!orgId || !membersTeam) throw new Error('No team selected')
      return teamApi.addMember(orgId, membersTeam.id, input)
    },
    [membersTeam, user?.orgId]
  )

  const updateSelectedTeamMember = useCallback(
    async (userId: string, input: Parameters<typeof teamApi.updateMember>[3]) => {
      const orgId = user?.orgId
      if (!orgId || !membersTeam) throw new Error('No team selected')
      return teamApi.updateMember(orgId, membersTeam.id, userId, input)
    },
    [membersTeam, user?.orgId]
  )

  const removeSelectedTeamMember = useCallback(
    async (userId: string) => {
      const orgId = user?.orgId
      if (!orgId || !membersTeam) throw new Error('No team selected')
      return teamApi.removeMember(orgId, membersTeam.id, userId)
    },
    [membersTeam, user?.orgId]
  )

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Teams and access groups</h2>
          <p className={uiStyles.sectionDescription}>
            {teams.length} {teams.length === 1 ? 'team groups people' : 'teams group people'} and
            projects inside this organization
          </p>
        </div>
        {!showForm && canCreateTeam && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <Plus size={14} strokeWidth={2} aria-hidden="true" />
            <span>New Team</span>
          </button>
        )}
      </div>

      {error && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {error}
        </div>
      )}

      <div className={cn(uiStyles.card)}>
        {loading && teams.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading teams…
          </div>
        ) : !user?.orgId ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              Choose an organization first
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Teams belong to an organization. Select or create one before adding people.
            </p>
          </div>
        ) : teams.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              {canCreateTeam
                ? 'Create a team first'
                : 'Ask an owner or admin to create the first team'}
            </p>
            <p className="mx-auto mt-1 max-w-sm text-ui-caption text-secondary-light dark:text-secondary-dark">
              {canCreateTeam
                ? 'Teams group projects and decide who can manage work. Start with one team, then add projects inside it.'
                : 'Only owners and admins can create teams. You can work here after someone adds a team for you.'}
            </p>
          </div>
        ) : (
          teams.map((team) => (
            <EditableTeamRow
              key={team.id}
              team={team}
              onUpdate={handleUpdate}
              onDelete={handleDelete}
              onManageMembers={setMembersTeam}
            />
          ))
        )}

        {showForm && canCreateTeam && (
          <CreateTeamForm
            onSave={handleCreate}
            onCancel={() => setShowForm(false)}
            saving={saving}
          />
        )}
      </div>

      {membersTeam && user?.orgId && (
        <ResourceMembersModal
          resourceLabel="Team"
          resourceName={membersTeam.name}
          loadMembers={loadSelectedTeamMembers}
          loadUsers={loadOrgUsers}
          addMember={addSelectedTeamMember}
          updateMember={updateSelectedTeamMember}
          removeMember={removeSelectedTeamMember}
          onClose={() => setMembersTeam(null)}
        />
      )}
    </div>
  )
}
