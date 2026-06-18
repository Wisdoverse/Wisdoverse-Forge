import { useState, type FormEvent } from 'react'
import { Check, Pencil, Trash2, Users, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { workspaceResourceErrorMessage } from '@app/shared/lib/workspaceResourceErrorMessage'
import type { CloneSummary, NavProject, UpdateProjectInput } from '@app/entities/project'
import { CloneStatusBadge } from './CloneStatusBadge'

const EMPTY_PROJECT_NAME_MESSAGE = 'Enter a project name, then save again.'

interface EditableProjectRowProps {
  project: NavProject
  teamName: string
  onUpdate: (project: NavProject, input: UpdateProjectInput) => Promise<void>
  onDelete: (project: NavProject) => Promise<void>
  onManageMembers?: (project: NavProject) => void
  onCloneRetried?: (projectId: string, summary: CloneSummary) => void
}

export function EditableProjectRow({
  project,
  teamName,
  onUpdate,
  onDelete,
  onManageMembers,
  onCloneRetried,
}: EditableProjectRowProps) {
  const canManage = project.canManage !== false
  const canDelete = project.canDelete !== false
  const [editing, setEditing] = useState(false)
  const [name, setName] = useState(project.name)
  const [description, setDescription] = useState(project.description ?? '')
  const [color, setColor] = useState(project.color || '#0066cc')
  const [saving, setSaving] = useState(false)
  const [confirmingDelete, setConfirmingDelete] = useState(false)
  const [error, setError] = useState<string | null>(null)

  function cancelEdit() {
    setEditing(false)
    setName(project.name)
    setDescription(project.description ?? '')
    setColor(project.color || '#0066cc')
    setError(null)
  }

  async function handleSave(e: FormEvent) {
    e.preventDefault()
    const trimmedName = name.trim()
    if (!trimmedName) {
      setError(EMPTY_PROJECT_NAME_MESSAGE)
      return
    }

    setSaving(true)
    setError(null)
    try {
      await onUpdate(project, { name: trimmedName, description: description.trim(), color })
      setEditing(false)
    } catch (err) {
      setError(workspaceResourceErrorMessage('project', 'update', err))
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
      await onDelete(project)
    } catch (err) {
      setError(workspaceResourceErrorMessage('project', 'delete', err))
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
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-[auto_1fr_1fr_auto] sm:items-start">
            <input
              type="color"
              value={color}
              onChange={(e) => setColor(e.target.value)}
              disabled={saving}
              aria-label="Project color"
              className="h-8 w-10 rounded-lg border border-black/10 bg-transparent p-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/30 disabled:cursor-not-allowed disabled:opacity-50 dark:border-white/10"
            />
            <input
              value={name}
              onChange={(e) => {
                const nextName = e.target.value
                setName(nextName)
                if (!nextName.trim()) {
                  setError(EMPTY_PROJECT_NAME_MESSAGE)
                } else if (error === EMPTY_PROJECT_NAME_MESSAGE) {
                  setError(null)
                }
              }}
              disabled={saving}
              autoFocus
              aria-label="Project name"
              className={uiStyles.input}
            />
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              disabled={saving}
              aria-label="Project description"
              placeholder="What work belongs here"
              className={uiStyles.input}
            />
            <div className="flex justify-end gap-1">
              <button
                type="button"
                onClick={cancelEdit}
                disabled={saving}
                aria-label="Cancel project edit"
                title="Cancel"
                className="flex h-8 w-8 items-center justify-center rounded-lg text-ui-button text-secondary-light transition-colors hover:bg-black/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/5"
              >
                <X size={14} strokeWidth={2} aria-hidden="true" />
              </button>
              <button
                type="submit"
                disabled={saving || !name.trim()}
                aria-label="Save project"
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
      <div className="flex items-center gap-3 min-w-0 flex-1">
        <div
          className="h-3 w-3 shrink-0 rounded-full ring-2 ring-black/5 dark:ring-white/10"
          style={{ backgroundColor: project.color || '#0066cc' }}
          aria-hidden="true"
        />
        <div className="min-w-0">
          <p className="truncate text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
            {project.name}
          </p>
          <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {teamName}
            {project.description ? ` · ${project.description}` : ''}
          </p>
          {project.cloneStatus && project.cloneStatus !== 'none' && (
            <CloneStatusBadge
              projectId={project.id}
              status={project.cloneStatus}
              clone={project.clone}
              variant="detail"
              onRetried={(summary) => onCloneRetried?.(project.id, summary)}
              className="mt-1.5"
            />
          )}
          {confirmingDelete && (
            <p className="mt-1 text-ui-caption font-medium text-apple-red" aria-live="polite">
              Click Delete project to confirm. Agents assigned here will be moved out of this
              project.
            </p>
          )}
          {error && (
            <p role="alert" className="mt-1 text-ui-caption text-apple-red">
              {error}
            </p>
          )}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <span className="hidden rounded-badge border border-black/5 bg-black/[0.03] px-1.5 py-0.5 text-[10px] text-secondary-light dark:border-white/10 dark:bg-white/[0.05] dark:text-secondary-dark sm:inline">
          Automatic link name: {project.slug}
        </span>
        {canManage && (
          <>
            <button
              type="button"
              onClick={() => {
                onManageMembers?.(project)
                setConfirmingDelete(false)
              }}
              disabled={saving || !onManageMembers}
              aria-label={`Manage members for ${project.name}`}
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
              aria-label={`Edit ${project.name}`}
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
            aria-label={`Delete ${project.name}`}
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
              aria-label={`Keep ${project.name}`}
              className="flex h-8 touch-manipulation items-center justify-center rounded-lg px-2 text-ui-button text-secondary-light transition-colors hover:bg-black/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35 disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/5"
            >
              Keep
            </button>
            <button
              type="button"
              onClick={() => void handleDelete()}
              disabled={saving}
              aria-label={`Confirm delete ${project.name}`}
              title="Confirm delete"
              className="flex h-8 w-auto touch-manipulation items-center justify-center whitespace-nowrap rounded-lg bg-apple-red px-2 text-ui-caption font-semibold text-white transition-colors hover:bg-apple-red/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-500/30 disabled:cursor-not-allowed disabled:opacity-50"
            >
              Delete project
            </button>
          </>
        )}
      </div>
    </div>
  )
}
