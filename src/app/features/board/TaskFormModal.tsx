import { useForm } from 'react-hook-form'
import { useEffect, useMemo, useRef, useState } from 'react'
import { AlertTriangle, FolderKanban, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'

interface TaskFormData {
  projectId: string
  title: string
  description: string
  priority: 'low' | 'normal' | 'high' | 'urgent'
  assignedTo: string
}

export interface TaskProjectOption {
  id: string
  name: string
  teamId: string
  teamName: string
  color?: string
}

interface TaskProjectGroup {
  teamId: string
  teamName: string
  projects: TaskProjectOption[]
}

interface TaskFormModalProps {
  isOpen: boolean
  onClose: () => void
  onSubmit: (data: TaskFormData) => void | Promise<void>
  agents?: { id: string; name: string; status: string }[]
  projects?: TaskProjectOption[]
  selectedProjectId?: string | null
  onProjectChange?: (projectId: string) => void | Promise<void>
}

export function TaskFormModal({
  isOpen,
  onClose,
  onSubmit,
  agents = [],
  projects = [],
  selectedProjectId = null,
  onProjectChange,
}: TaskFormModalProps) {
  const {
    register,
    handleSubmit,
    reset,
    setValue,
    watch,
    formState: { isSubmitting },
  } = useForm<TaskFormData>({
    defaultValues: {
      projectId: selectedProjectId ?? '',
      title: '',
      description: '',
      priority: 'normal',
      assignedTo: '',
    },
  })
  const [submitError, setSubmitError] = useState<string | null>(null)
  const [selectingProject, setSelectingProject] = useState(false)

  const dialogRef = useRef<HTMLDivElement>(null)
  const projectId = watch('projectId')
  const selectedProject = projects.find((project) => project.id === projectId)
  const projectGroups = useMemo(() => groupProjectsByTeam(projects), [projects])
  const projectField = register('projectId', {
    required: 'Choose a project before creating a task.',
  })

  useEffect(() => {
    if (isOpen) setSubmitError(null)
  }, [isOpen])

  useEffect(() => {
    if (isOpen) setValue('projectId', selectedProjectId ?? '')
  }, [isOpen, selectedProjectId, setValue])

  useEffect(() => {
    if (!isOpen) return
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [isOpen, onClose])

  if (!isOpen) return null

  async function handleFormSubmit(data: TaskFormData) {
    setSubmitError(null)
    if (!data.projectId) {
      setSubmitError('Choose a project before creating a task.')
      return
    }
    try {
      await onSubmit(data)
      reset()
      onClose()
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : 'Failed to create task')
    }
  }

  async function handleProjectChange(projectId: string) {
    setSubmitError(null)
    if (!projectId || !onProjectChange) return
    setSelectingProject(true)
    try {
      await onProjectChange(projectId)
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : 'Failed to select project')
    } finally {
      setSelectingProject(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="task-form-title"
        className={cn(
          'relative max-h-[80vh] w-[480px] max-w-[calc(100vw-24px)] overflow-y-auto',
          'rounded-panel border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-[#2c2c2e]'
        )}
      >
        <div className="flex items-center justify-between mb-4">
          <h2 id="task-form-title" className="text-ui-title font-semibold">
            New Task
          </h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close dialog"
            className="flex h-7 w-7 items-center justify-center rounded-lg text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
          >
            <X size={14} strokeWidth={2} />
          </button>
        </div>

        {projects.length === 0 ? (
          <div className="mb-4 flex gap-2 rounded-lg bg-apple-orange/10 px-3 py-2 text-ui-caption text-apple-orange">
            <AlertTriangle
              size={14}
              strokeWidth={2}
              className="mt-0.5 shrink-0"
              aria-hidden="true"
            />
            <span>No projects available. Create a project in Settings before creating tasks.</span>
          </div>
        ) : (
          <div className="mb-4">
            <label
              htmlFor="task-project"
              className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
            >
              Project
            </label>
            <select
              id="task-project"
              {...projectField}
              onChange={(event) => {
                void projectField.onChange(event)
                void handleProjectChange(event.target.value)
              }}
              disabled={isSubmitting || selectingProject}
              className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus disabled:opacity-60 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
            >
              <option value="">Select a project…</option>
              {projectGroups.map((group) => (
                <optgroup key={group.teamId} label={group.teamName}>
                  {group.projects.map((project) => (
                    <option key={project.id} value={project.id}>
                      {project.name}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
            <div
              className={cn(
                'mt-2 flex items-center gap-2 rounded-lg px-3 py-2 text-ui-caption',
                selectedProject
                  ? 'bg-apple-blue/10 text-foreground-light dark:text-foreground-dark'
                  : 'bg-apple-orange/10 text-apple-orange'
              )}
            >
              <FolderKanban size={14} strokeWidth={2} className="shrink-0" aria-hidden="true" />
              {selectedProject ? (
                <>
                  <span
                    className="h-2 w-2 shrink-0 rounded-full"
                    style={{ backgroundColor: selectedProject.color || '#007AFF' }}
                  />
                  <span className="min-w-0 truncate">
                    {selectedProject.teamName} / {selectedProject.name}
                  </span>
                </>
              ) : (
                <span>Select the project that should own this task.</span>
              )}
            </div>
          </div>
        )}

        {agents.length === 0 && (
          <div className="mb-4 flex gap-2 rounded-lg bg-apple-orange/10 px-3 py-2 text-ui-caption text-apple-orange">
            <AlertTriangle
              size={14}
              strokeWidth={2}
              className="mt-0.5 shrink-0"
              aria-hidden="true"
            />
            <span>
              No online agents available. Tasks will be queued and dispatched when an agent comes
              online.
            </span>
          </div>
        )}

        {submitError && (
          <div
            role="alert"
            className="mb-4 rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
          >
            {submitError}
          </div>
        )}

        <form onSubmit={handleSubmit(handleFormSubmit)} className="flex flex-col gap-4">
          <div>
            <label
              htmlFor="task-title"
              className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
            >
              Title
            </label>
            <input
              id="task-title"
              autoComplete="off"
              {...register('title', { required: true })}
              className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              placeholder="What needs to be done…"
              autoFocus
            />
          </div>

          <div>
            <label
              htmlFor="task-description"
              className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
            >
              Description
            </label>
            <textarea
              id="task-description"
              autoComplete="off"
              {...register('description')}
              rows={3}
              className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              placeholder="Additional details…"
            />
          </div>

          <div className="flex flex-col gap-4 sm:flex-row">
            <div className="flex-1">
              <label
                htmlFor="task-priority"
                className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
              >
                Priority
              </label>
              <select
                id="task-priority"
                {...register('priority')}
                className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              >
                <option value="low">Low</option>
                <option value="normal">Normal</option>
                <option value="high">High</option>
                <option value="urgent">Urgent</option>
              </select>
            </div>
            <div className="flex-1">
              <label
                htmlFor="task-assigned-to"
                className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
              >
                Assign Agent
              </label>
              <select
                id="task-assigned-to"
                {...register('assignedTo')}
                className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              >
                <option value="">Unassigned</option>
                {agents.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name} ({a.status})
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="mt-2 flex justify-end gap-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded-full bg-surface-pearl px-4 py-2 text-ui-button font-medium text-foreground-light ring-1 ring-black/[0.04] transition-transform active:scale-95 dark:bg-white/[0.06] dark:text-foreground-dark"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting || selectingProject || projects.length === 0 || !projectId}
              aria-busy={isSubmitting || selectingProject}
              className="rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 disabled:cursor-not-allowed disabled:opacity-60"
            >
              {selectingProject ? 'Selecting…' : isSubmitting ? 'Creating…' : 'Create Task'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

function groupProjectsByTeam(projects: TaskProjectOption[]): TaskProjectGroup[] {
  const groups: TaskProjectGroup[] = []
  const indexByTeamId = new Map<string, number>()

  for (const project of projects) {
    const existingIndex = indexByTeamId.get(project.teamId)
    if (existingIndex !== undefined) {
      groups[existingIndex].projects.push(project)
      continue
    }

    indexByTeamId.set(project.teamId, groups.length)
    groups.push({
      teamId: project.teamId,
      teamName: project.teamName,
      projects: [project],
    })
  }

  return groups
}
