import { useState, type FormEvent } from 'react'
import { Check, Pencil, Trash2, Users, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { workspaceResourceErrorMessage } from '@app/shared/lib/workspaceResourceErrorMessage'
import type { CloneSummary, NavProject, UpdateProjectInput } from '@app/entities/navigation/project'
import { CloneStatusBadge } from './CloneStatusBadge'

const EMPTY_PROJECT_NAME_MESSAGE = 'Enter a project name, then save this project name again.'
const PROJECT_DELETE_CONFIRMATION_MESSAGE =
  'Delete this project from Settings and the left menu. Agents using this project will be moved out of it. Choose Keep if you are not sure.'

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
            <div role="alert" aria-live="polite" className={uiStyles.error}>
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
              className="h-8 w-10 rounded-button border border-black/10 bg-transparent p-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/30 disabled:cursor-not-allowed disabled:opacity-50 dark:border-white/10"
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
                className={cn(uiStyles.subtleButton, 'w-8 px-0')}
              >
                <X size={14} strokeWidth={2} aria-hidden="true" />
              </button>
              <button
                type="submit"
                disabled={saving || !name.trim()}
                aria-label="Save project"
                title="Save"
                className={cn(uiStyles.primaryButton, 'w-8 px-0')}
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
              {PROJECT_DELETE_CONFIRMATION_MESSAGE}
            </p>
          )}
          {error && (
            <p role="alert" aria-live="polite" className="mt-1 text-ui-caption text-apple-red">
              {error}
            </p>
          )}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <span className={cn(uiStyles.chip, 'hidden sm:inline-flex')}>
          Project link preview: {project.slug}. Forge creates this automatically from the project
          name
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
              aria-label={`Manage people and access for ${project.name}`}
              title="Manage people and access"
              className={cn(uiStyles.subtleButton, 'w-8 touch-manipulation px-0')}
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
              title="Rename project"
              className={cn(uiStyles.subtleButton, 'w-8 touch-manipulation px-0')}
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
            title="Delete project"
            className={cn(uiStyles.dangerButton, 'w-8 touch-manipulation px-0')}
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
              className={cn(uiStyles.subtleButton, 'touch-manipulation')}
            >
              Keep
            </button>
            <button
              type="button"
              onClick={() => void handleDelete()}
              disabled={saving}
              aria-label={`Confirm delete ${project.name}`}
              title="Confirm delete"
              className={cn(
                uiStyles.secondaryButton,
                'touch-manipulation border-apple-red/30 text-apple-red dark:border-apple-red/30 dark:text-apple-red'
              )}
            >
              Delete project
            </button>
          </>
        )}
      </div>
    </div>
  )
}
