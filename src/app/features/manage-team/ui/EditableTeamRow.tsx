import { useState, type FormEvent } from 'react'
import { Check, Pencil, Trash2, Users, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { workspaceResourceErrorMessage } from '@app/shared/lib/workspaceResourceErrorMessage'
import type { NavTeam, UpdateTeamInput } from '@app/entities/team'

const EMPTY_TEAM_NAME_MESSAGE = 'Enter a team name, then save again.'

interface EditableTeamRowProps {
  team: NavTeam
  onUpdate: (teamId: string, input: UpdateTeamInput) => Promise<void>
  onDelete: (teamId: string) => Promise<void>
  onManageMembers?: (team: NavTeam) => void
}

export function EditableTeamRow({
  team,
  onUpdate,
  onDelete,
  onManageMembers,
}: EditableTeamRowProps) {
  const canManage = team.canManage !== false
  const canDelete = team.canDelete !== false
  const [editing, setEditing] = useState(false)
  const [name, setName] = useState(team.name)
  const [description, setDescription] = useState(team.description ?? '')
  const [saving, setSaving] = useState(false)
  const [confirmingDelete, setConfirmingDelete] = useState(false)
  const [error, setError] = useState<string | null>(null)

  function cancelEdit() {
    setEditing(false)
    setName(team.name)
    setDescription(team.description ?? '')
    setError(null)
  }

  async function handleSave(e: FormEvent) {
    e.preventDefault()
    const trimmedName = name.trim()
    if (!trimmedName) {
      setError(EMPTY_TEAM_NAME_MESSAGE)
      return
    }

    setSaving(true)
    setError(null)
    try {
      await onUpdate(team.id, { name: trimmedName, description: description.trim() })
      setEditing(false)
    } catch (err) {
      setError(workspaceResourceErrorMessage('team', 'update', err))
    } finally {
      setSaving(false)
    }
  }

  async function handleDelete() {
    if (!confirmingDelete) {
      setConfirmingDelete(true)
      return
    }

    setSaving(true)
    setError(null)
    try {
      await onDelete(team.id)
    } catch (err) {
      setError(workspaceResourceErrorMessage('team', 'delete', err))
      setSaving(false)
      setConfirmingDelete(false)
    }
  }

  if (editing) {
    return (
      <form
        onSubmit={handleSave}
        className={cn(
          'border-b border-black/[0.06] px-4 py-3 last:border-b-0 dark:border-white/[0.08]',
          'bg-black/[0.015] dark:bg-white/[0.025]'
        )}
      >
        <div className="flex flex-col gap-2">
          {error && (
            <div role="alert" className={uiStyles.error}>
              {error}
            </div>
          )}
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-[1fr_1fr_auto] sm:items-start">
            <input
              value={name}
              onChange={(e) => {
                const nextName = e.target.value
                setName(nextName)
                if (!nextName.trim()) {
                  setError(EMPTY_TEAM_NAME_MESSAGE)
                } else if (error === EMPTY_TEAM_NAME_MESSAGE) {
                  setError(null)
                }
              }}
              disabled={saving}
              autoFocus
              aria-label="Team name"
              className={uiStyles.input}
            />
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              disabled={saving}
              aria-label="Team description"
              placeholder="What this team owns"
              className={uiStyles.input}
            />
            <div className="flex justify-end gap-1">
              <button
                type="button"
                onClick={cancelEdit}
                disabled={saving}
                aria-label="Cancel team edit"
                title="Cancel"
                className="flex h-8 w-8 items-center justify-center rounded-lg text-ui-button text-secondary-light transition-colors hover:bg-black/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/5"
              >
                <X size={14} strokeWidth={2} aria-hidden="true" />
              </button>
              <button
                type="submit"
                disabled={saving || !name.trim()}
                aria-label="Save team"
                title="Save"
                className="flex h-8 w-8 items-center justify-center rounded-lg bg-apple-blue text-ui-button text-white transition-colors hover:bg-apple-blue/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/40 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <Check size={14} strokeWidth={2} aria-hidden="true" />
              </button>
            </div>
          </div>
        </div>
      </form>
    )
  }

  return (
    <div
      className={cn(
        'group flex items-center justify-between gap-3 px-4 py-3',
        'border-b border-black/[0.06] transition-colors last:border-b-0 hover:bg-black/[0.015]',
        'dark:border-white/[0.08] dark:hover:bg-white/[0.025]'
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <p className="truncate text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
            {team.name}
          </p>
          <span
            className={cn(
              'shrink-0',
              team.visibility === 'open' ? uiStyles.activeBadge : uiStyles.badge
            )}
          >
            {team.visibility}
          </span>
        </div>
        <p className="mt-0.5 truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          Automatic link name: {team.slug}
          {team.description ? ` · ${team.description}` : ''}
        </p>
        {confirmingDelete && (
          <p className="mt-1 text-ui-caption font-medium text-apple-red" aria-live="polite">
            Click Delete team to confirm. Projects in this team will also disappear from the left
            menu.
          </p>
        )}
        {error && (
          <p role="alert" className="mt-1 text-ui-caption text-apple-red">
            {error}
          </p>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {canManage && (
          <>
            <button
              type="button"
              onClick={() => {
                onManageMembers?.(team)
                setConfirmingDelete(false)
              }}
              disabled={saving || !onManageMembers}
              aria-label={`Manage members for ${team.name}`}
              title="Members"
              className="flex h-8 w-8 touch-manipulation items-center justify-center rounded-lg text-ui-button text-secondary-light transition-colors hover:bg-apple-blue/10 hover:text-apple-blue focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-apple-blue/15 dark:hover:text-apple-blue"
            >
              <Users size={14} strokeWidth={2} aria-hidden="true" />
            </button>
            <button
              type="button"
              onClick={() => {
                setEditing(true)
                setConfirmingDelete(false)
              }}
              disabled={saving}
              aria-label={`Edit ${team.name}`}
              title="Edit"
              className="flex h-8 w-8 touch-manipulation items-center justify-center rounded-lg text-ui-button text-secondary-light transition-colors hover:bg-black/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/5"
            >
              <Pencil size={14} strokeWidth={2} aria-hidden="true" />
            </button>
          </>
        )}
        {canDelete && !confirmingDelete && (
          <button
            type="button"
            onClick={() => void handleDelete()}
            disabled={saving}
            aria-label={`Delete ${team.name}`}
            title="Delete"
            className="flex h-8 w-8 touch-manipulation items-center justify-center rounded-lg text-ui-button text-secondary-light transition-colors hover:bg-apple-red/10 hover:text-apple-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-500/30 disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-apple-red/10 dark:hover:text-apple-red"
          >
            <Trash2 size={14} strokeWidth={2} aria-hidden="true" />
          </button>
        )}
        {canDelete && confirmingDelete && (
          <>
            <button
              type="button"
              onClick={() => setConfirmingDelete(false)}
              disabled={saving}
              aria-label={`Keep ${team.name}`}
              className="flex h-8 touch-manipulation items-center justify-center rounded-lg px-2 text-ui-button text-secondary-light transition-colors hover:bg-black/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/5"
            >
              Keep
            </button>
            <button
              type="button"
              onClick={() => void handleDelete()}
              disabled={saving}
              aria-label={`Confirm delete ${team.name}`}
              title="Confirm delete"
              className="flex h-8 w-auto touch-manipulation items-center justify-center whitespace-nowrap rounded-lg bg-apple-red px-2 text-ui-caption font-semibold text-white transition-colors hover:bg-apple-red/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-500/30 disabled:cursor-not-allowed disabled:opacity-50"
            >
              Delete team
            </button>
          </>
        )}
      </div>
    </div>
  )
}
