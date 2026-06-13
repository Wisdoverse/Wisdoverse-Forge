import { useForm } from 'react-hook-form'
import { useEffect, useMemo, useRef, useState } from 'react'
import {
  AlertTriangle,
  Bug,
  ClipboardCheck,
  FolderKanban,
  Search,
  ShieldCheck,
  X,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { boardActionErrorMessage } from './boardErrorMessages'

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

interface TaskBriefTemplate {
  id: string
  label: string
  summary: string
  title: string
  description: string
  priority: TaskFormData['priority']
  Icon: LucideIcon
}

const TASK_BRIEF_TEMPLATES: TaskBriefTemplate[] = [
  {
    id: 'feature',
    label: 'Feature',
    summary: 'Build a contained change',
    title: 'Build a focused feature',
    description:
      'What should change:\n- \n\nWhere to work:\n- \n\nWhat to avoid:\n- \n\nDone when:\n- ',
    priority: 'normal',
    Icon: ClipboardCheck,
  },
  {
    id: 'bug',
    label: 'Bug',
    summary: 'Reproduce and fix',
    title: 'Fix a reproducible defect',
    description:
      'What is broken:\n- \n\nWhat should happen:\n- \n\nWhere to look first:\n- \n\nDone when:\n- ',
    priority: 'high',
    Icon: Bug,
  },
  {
    id: 'investigation',
    label: 'Investigate',
    summary: 'Find the reason',
    title: 'Investigate an unclear issue',
    description:
      'Question to answer:\n- \n\nWhat to inspect:\n- \n\nWhat is already known:\n- \n\nDecision needed:\n- ',
    priority: 'normal',
    Icon: Search,
  },
  {
    id: 'review',
    label: 'Review',
    summary: 'Check before release',
    title: 'Review a change for release readiness',
    description:
      'Change to review:\n- \n\nWhat could go wrong:\n- \n\nChecks to run:\n- \n\nAnswer format:\n- ',
    priority: 'normal',
    Icon: ShieldCheck,
  },
]

const AGENT_READY_BRIEF_POINTS = [
  { label: 'What to finish', value: 'The visible change or decision you need.' },
  { label: 'Where to work', value: 'Files, screens, or areas to use and what to avoid.' },
  { label: 'Done when', value: 'The check, screenshot, or output that proves it is done.' },
]

interface TaskFormModalProps {
  isOpen: boolean
  onClose: () => void
  onSubmit: (data: TaskFormData) => void | Promise<void>
  agents?: { id: string; name: string; status: string }[]
  projects?: TaskProjectOption[]
  selectedProjectId?: string | null
  selectedTaskGroupId?: string | null
  selectedTaskGroupName?: string | null
  /** May resolve `false` to signal the project switched but its work lanes
   * failed to load (the modal shows a retry message in that case). */
  onProjectChange?: (projectId: string) => void | boolean | Promise<void | boolean>
  onOpenTaskRouting?: () => void
}

export function TaskFormModal({
  isOpen,
  onClose,
  onSubmit,
  agents = [],
  projects = [],
  selectedProjectId = null,
  selectedTaskGroupId = null,
  selectedTaskGroupName = null,
  onProjectChange,
  onOpenTaskRouting,
}: TaskFormModalProps) {
  const {
    register,
    handleSubmit,
    reset,
    setValue,
    watch,
    formState: { errors, isSubmitting, submitCount },
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
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(null)

  const dialogRef = useRef<HTMLDivElement>(null)
  const errorBannerRef = useRef<HTMLDivElement>(null)
  const projectId = watch('projectId')
  const selectedProject = projects.find((project) => project.id === projectId)
  const projectSelectionSettled = Boolean(projectId && selectedProjectId === projectId)
  const workLaneReady = Boolean(projectSelectionSettled && selectedTaskGroupId)
  const assignableAgents = agents.filter((agent) => agentCanTakeTask(agent.status))
  const projectGroups = useMemo(() => groupProjectsByTeam(projects), [projects])
  const projectField = register('projectId')

  // The error banner renders partway down a scrollable dialog (below the
  // header and project panels) while the submit button sits at the bottom, so
  // a failed submit can leave the banner off-screen and look like a dead
  // click. Scroll the banner itself into view, and include `submitCount` so a
  // repeat submit with the SAME message scrolls again.
  useEffect(() => {
    if (submitError)
      errorBannerRef.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [submitError, submitCount])

  useEffect(() => {
    if (isOpen) {
      setSubmitError(null)
      setSelectedTemplateId(null)
    }
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
    if (!data.title.trim()) {
      setSubmitError('Add a short title so the agent knows the goal.')
      return
    }
    if (!data.projectId) {
      setSubmitError('Choose a project before creating a task.')
      return
    }
    if (!selectedTaskGroupId) {
      setSubmitError('Create a task queue before creating a task.')
      return
    }
    try {
      await onSubmit({ ...data, title: data.title.trim() })
    } catch (err) {
      setSubmitError(boardActionErrorMessage('createTask', err))
      return
    }
    reset()
    onClose()
  }

  async function handleProjectChange(projectId: string) {
    setSubmitError(null)
    if (!projectId || !onProjectChange) return
    setSelectingProject(true)
    try {
      const ok = await onProjectChange(projectId)
      if (ok === false) {
        setSubmitError('Could not load work lanes for this project. Select it again to retry.')
      }
    } catch (err) {
      setSubmitError(boardActionErrorMessage('selectProject', err))
    } finally {
      setSelectingProject(false)
    }
  }

  function applyTemplate(template: TaskBriefTemplate) {
    setSelectedTemplateId(template.id)
    setValue('title', template.title, { shouldDirty: true })
    setValue('description', template.description, { shouldDirty: true })
    setValue('priority', template.priority, { shouldDirty: true })
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
          <div className="min-w-0">
            <h2 id="task-form-title" className="text-ui-title font-semibold">
              Tell an Agent What to Do
            </h2>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Write the result you want. A template can add what to include and how to check the
              work.
            </p>
          </div>
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
                <span>Choose where this task, its files, and its activity history belong.</span>
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
              No agents are online. You can create the task now; it will wait here until an agent
              comes online.
            </span>
          </div>
        )}

        {agents.length > 0 && assignableAgents.length === 0 && (
          <div className="mb-4 flex gap-2 rounded-lg bg-apple-orange/10 px-3 py-2 text-ui-caption text-apple-orange">
            <AlertTriangle
              size={14}
              strokeWidth={2}
              className="mt-0.5 shrink-0"
              aria-hidden="true"
            />
            <span>
              No agents are available right now. Keep the default choice so the next available agent
              can pick it up.
            </span>
          </div>
        )}

        {selectedProject && (
          <div
            data-testid="task-work-lane-readiness"
            className={cn(
              'mb-4 rounded-lg border px-3 py-3 text-ui-caption',
              workLaneReady
                ? 'border-apple-blue/20 bg-apple-blue/10 text-foreground-light dark:text-foreground-dark'
                : 'border-apple-orange/20 bg-apple-orange/10 text-apple-orange'
            )}
          >
            <div className="flex gap-2">
              {workLaneReady ? (
                <FolderKanban
                  size={15}
                  strokeWidth={2}
                  className="mt-0.5 shrink-0"
                  aria-hidden="true"
                />
              ) : (
                <AlertTriangle
                  size={15}
                  strokeWidth={2}
                  className="mt-0.5 shrink-0"
                  aria-hidden="true"
                />
              )}
              <div className="min-w-0 flex-1">
                <p className="font-semibold">
                  {workLaneReady ? 'Ready to Send' : 'Create a Task Queue First'}
                </p>
                <p className="mt-0.5 text-secondary-light dark:text-secondary-dark">
                  {workLaneReady
                    ? `New tasks will wait in ${selectedTaskGroupName ?? 'this task queue'} until an available agent picks them up.`
                    : 'A task queue gives new work a place to wait. Create one once, then return here.'}
                </p>
              </div>
            </div>
            {!workLaneReady && onOpenTaskRouting && (
              <button
                type="button"
                onClick={onOpenTaskRouting}
                className="mt-3 inline-flex h-8 items-center justify-center rounded-full border border-apple-orange/30 bg-white px-3 text-ui-button font-medium text-apple-orange transition-colors hover:bg-apple-orange/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-orange/35 dark:bg-white/[0.06]"
              >
                Open Task Queues
              </button>
            )}
          </div>
        )}

        {submitError && (
          <div
            ref={errorBannerRef}
            role="alert"
            className="mb-4 rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
          >
            {submitError}
          </div>
        )}

        <form noValidate onSubmit={handleSubmit(handleFormSubmit)} className="flex flex-col gap-4">
          <div>
            <div className="mb-2 flex items-center justify-between gap-2">
              <span className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Start with a task template
              </span>
              <span className="hidden text-ui-caption text-secondary-light dark:text-secondary-dark sm:inline">
                Fills in a safe first draft
              </span>
            </div>
            <div role="group" aria-label="Task templates" className="grid gap-2 sm:grid-cols-2">
              {TASK_BRIEF_TEMPLATES.map((template) => (
                <button
                  key={template.id}
                  type="button"
                  onClick={() => applyTemplate(template)}
                  aria-pressed={selectedTemplateId === template.id}
                  className={cn(
                    'flex min-h-16 items-center gap-3 rounded-lg border px-3 py-2 text-left transition-colors',
                    selectedTemplateId === template.id
                      ? 'border-apple-blue/40 bg-apple-blue/10 text-foreground-light dark:text-foreground-dark'
                      : 'border-black/[0.08] bg-black/[0.02] text-foreground-light hover:bg-black/[0.04] dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.07]'
                  )}
                >
                  <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-white text-apple-blue shadow-sm dark:bg-black/20">
                    <template.Icon size={15} strokeWidth={2.25} aria-hidden="true" />
                  </span>
                  <span className="min-w-0">
                    <span className="block text-ui-button font-semibold">{template.label}</span>
                    <span className="block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                      {template.summary}
                    </span>
                  </span>
                </button>
              ))}
            </div>
            <div className="mt-3 rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2.5 dark:border-white/[0.08] dark:bg-white/[0.04]">
              <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                A clear task has three parts
              </div>
              <div className="mt-2 grid gap-1.5 sm:grid-cols-3">
                {AGENT_READY_BRIEF_POINTS.map((point) => (
                  <div
                    key={point.label}
                    className="min-w-0 rounded-md bg-white px-2 py-1.5 dark:bg-black/20"
                  >
                    <span className="block text-[10px] font-medium text-secondary-light dark:text-secondary-dark">
                      {point.label}
                    </span>
                    <span className="mt-0.5 block text-ui-caption text-foreground-light dark:text-foreground-dark">
                      {point.value}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </div>

          <div>
            <label
              htmlFor="task-title"
              className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
            >
              What should the agent finish?
            </label>
            <input
              id="task-title"
              autoComplete="off"
              {...register('title', { required: 'Add a short title so the agent knows the goal.' })}
              aria-invalid={errors.title ? 'true' : undefined}
              aria-describedby={errors.title ? 'task-title-error' : 'task-title-help'}
              className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              placeholder="For example: Fix the login error"
              autoFocus
            />
            {errors.title ? (
              <p id="task-title-error" role="alert" className="mt-1 text-ui-caption text-apple-red">
                {errors.title.message}
              </p>
            ) : (
              <p
                id="task-title-help"
                className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
              >
                Use one sentence. Put the details in the next field.
              </p>
            )}
          </div>

          <div>
            <label
              htmlFor="task-description"
              className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
            >
              Details the agent should know
            </label>
            <textarea
              id="task-description"
              autoComplete="off"
              {...register('description')}
              rows={3}
              className="w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-4 py-3 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              placeholder="Add background, files to check, what to avoid, and how you will know it is done."
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
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Normal is right for most work. Use Urgent only when people are waiting on it now.
              </p>
            </div>
            <div className="flex-1">
              <div className="mb-1 flex items-center justify-between gap-2">
                <label
                  htmlFor="task-assigned-to"
                  className="block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                >
                  Who should pick it up?
                </label>
                <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {assignableAgents.length} available
                </span>
              </div>
              <select
                id="task-assigned-to"
                {...register('assignedTo')}
                className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
              >
                <option value="">Let the next available agent pick it up</option>
                {agents.map((a) => (
                  <option key={a.id} value={a.id} disabled={!agentCanTakeTask(a.status)}>
                    {a.name} ({agentStatusLabel(a.status)})
                  </option>
                ))}
              </select>
              <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                Keep this choice when any available agent can do the work.
              </p>
            </div>
          </div>

          <div className="mt-2 flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
            <button
              type="button"
              onClick={onClose}
              className="w-full rounded-full bg-surface-pearl px-4 py-2 text-ui-button font-medium text-foreground-light ring-1 ring-black/[0.04] transition-transform active:scale-95 dark:bg-white/[0.06] dark:text-foreground-dark sm:w-auto"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={
                isSubmitting ||
                selectingProject ||
                projects.length === 0 ||
                !projectId ||
                !selectedTaskGroupId
              }
              aria-busy={isSubmitting || selectingProject}
              className="w-full rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 disabled:cursor-not-allowed disabled:opacity-60 sm:w-auto"
            >
              {selectingProject ? 'Selecting…' : isSubmitting ? 'Creating…' : 'Create Task'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

function agentCanTakeTask(status: string): boolean {
  const normalized = normalizeAgentStatus(status)
  return normalized === 'available' || normalized === 'idle'
}

function agentStatusLabel(status: string): string {
  const normalized = normalizeAgentStatus(status)
  switch (normalized) {
    case 'available':
    case 'idle':
      return 'ready'
    case 'busy':
    case 'working':
      return 'busy'
    case 'offline':
      return 'offline'
    default:
      return normalized ? 'not ready' : 'status not reported'
  }
}

function normalizeAgentStatus(status: string): string {
  return status.trim().toLowerCase()
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
