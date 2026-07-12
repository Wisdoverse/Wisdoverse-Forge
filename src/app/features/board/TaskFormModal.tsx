import { useForm } from 'react-hook-form'
import { useEffect, useMemo, useRef, useState } from 'react'
import {
  AlertTriangle,
  Bug,
  ChevronDown,
  ChevronRight,
  ClipboardCheck,
  FolderKanban,
  ImagePlus,
  Search,
  ShieldCheck,
  X,
  type LucideIcon,
} from 'lucide-react'
import { waitingPlaceDisplayName } from '@app/entities/navigation/agent-group'
import { useAgentsStore, isTaskImageCapable } from '@app/entities/agent'
import { cn } from '@app/shared/lib/utils'
import { boardActionErrorMessage } from './boardErrorMessages'
import {
  agentCanTakeTask,
  agentHasTaskCapability,
  agentTaskStatusLabel,
} from './model/agentTaskReadiness'

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

interface TaskBriefCue {
  id: 'goal' | 'where' | 'done'
  label: string
  ready: boolean
  readyDetail: string
  missingDetail: string
}

interface TaskFormAgentOption {
  id: string
  name: string
  status: string
  capabilities?: string[]
  runtimeKind?: 'container' | 'cli' | 'api'
}

const TASK_BRIEF_TEMPLATES: TaskBriefTemplate[] = [
  {
    id: 'feature',
    label: 'Add something',
    summary: 'Add one clear change',
    title: 'Add one focused change',
    description:
      'What should change:\n- Describe what you want to see or use after this is done.\n\nWhere to work:\n- Name the page or area if you know it.\n\nWhat to avoid:\n- List anything that should stay unchanged.\n\nDone when:\n- Say what should be visible, ready to use, or easy to check.',
    priority: 'normal',
    Icon: ClipboardCheck,
  },
  {
    id: 'bug',
    label: 'Fix a problem',
    summary: 'Find what breaks and fix it',
    title: 'Fix a problem you can repeat',
    description:
      'What is broken:\n- Describe what you see now.\n\nWhat should happen:\n- Describe the correct result.\n\nWhere to look first:\n- Name the page or step where you saw it.\n\nDone when:\n- Say how you will know the problem is fixed.',
    priority: 'high',
    Icon: Bug,
  },
  {
    id: 'investigation',
    label: 'Find the cause',
    summary: 'Explain what is happening',
    title: 'Find the cause of an unclear problem',
    description:
      'Question to answer:\n- Write the question in one sentence.\n\nWhat to inspect:\n- Add pages, clues, links, screenshots, or recent changes if you know them.\n\nWhat is already known:\n- Add what you already tried or noticed.\n\nDecision needed:\n- Say what answer or recommendation you need.',
    priority: 'normal',
    Icon: Search,
  },
  {
    id: 'review',
    label: 'Check a change',
    summary: 'Look for risks before using it',
    title: 'Check whether a change is safe to use',
    description:
      'Change to check:\n- Name what changed and where a user would see it.\n\nWhat could go wrong:\n- List the risks you care about.\n\nWhat to check:\n- Say what you want the agent to check, such as a screen, result, or sign-in step.\n\nAnswer needed:\n- Ask for what is safe, what needs fixing, and what to do next.',
    priority: 'normal',
    Icon: ShieldCheck,
  },
]

const PRIORITY_LABELS: Record<TaskFormData['priority'], string> = {
  low: 'Low',
  normal: 'Normal',
  high: 'High',
  urgent: 'Urgent',
}

const AGENT_READY_BRIEF_POINTS = [
  { label: 'Goal', value: 'The visible change, answer, or decision you need.' },
  { label: 'Place', value: 'The page, project area, file, or step to check first.' },
  { label: 'Proof', value: 'The simple result that tells you the task is finished.' },
]

const PROJECT_REQUIRED_ERROR = 'Open project settings before creating a task.'
const TASK_WAITING_PLACE_REQUIRED_ERROR = 'Set up a task queue before saving this task.'
const ASSIGNED_AGENT_NOT_READY_ERROR =
  'Choose a ready agent, or leave this set to Let the next ready agent start it.'

interface TaskFormModalProps {
  isOpen: boolean
  onClose: () => void
  onSubmit: (data: TaskFormData & { imageAttachmentIds?: string[] }) => void | Promise<void>
  agents?: TaskFormAgentOption[]
  projects?: TaskProjectOption[]
  selectedProjectId?: string | null
  selectedTaskGroupId?: string | null
  selectedTaskGroupName?: string | null
  /** May resolve `false` to signal the project switched but its task queues
   * failed to load (the modal shows a retry message in that case). */
  onProjectChange?: (projectId: string) => void | boolean | Promise<void | boolean>
  onOpenAgentSetup?: () => void
  onOpenProjectSettings?: () => void
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
  onOpenAgentSetup,
  onOpenProjectSettings,
  onOpenTaskRouting,
}: TaskFormModalProps) {
  const {
    register,
    handleSubmit,
    reset,
    setFocus,
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
  const [confirmIncompleteBrief, setConfirmIncompleteBrief] = useState(false)
  const [taskTemplatesOpen, setTaskTemplatesOpen] = useState(true)
  const [taskOptionsOpen, setTaskOptionsOpen] = useState(false)
  const [imageIds, setImageIds] = useState<string[]>([])
  const [imagePreviews, setImagePreviews] = useState<{ id: string; name: string }[]>([])
  const [uploadingImage, setUploadingImage] = useState(false)
  const [imageError, setImageError] = useState<string | null>(null)
  const imageInputRef = useRef<HTMLInputElement>(null)
  // Live mirror of the assignee so an in-flight upload can detect a mid-upload
  // assignee change and discard a now-stale attachment id.
  const assignedToRef = useRef('')
  const uploadImage = useAgentsStore((state) => state.uploadImage)

  const dialogRef = useRef<HTMLDivElement>(null)
  const errorBannerRef = useRef<HTMLDivElement>(null)
  const projectId = watch('projectId')
  const selectedProject = projects.find((project) => project.id === projectId)
  const projectSelectionSettled = Boolean(projectId && selectedProjectId === projectId)
  const workLaneReady = Boolean(projectSelectionSettled && selectedTaskGroupId)
  const readinessTitle = selectingProject
    ? 'Checking where new tasks will wait'
    : workLaneReady
      ? 'Task can be created'
      : 'Set up a task queue before creating this task'
  const readinessDetail = selectingProject
    ? 'Wait a moment while Forge finds the task queue for this project.'
    : workLaneReady
      ? `New tasks will wait in ${waitingPlaceDisplayName(selectedTaskGroupName)} until a ready agent starts them.`
      : 'Create one place for new tasks to wait, then return here.'
  const waitingPlaceSetupSteps =
    selectedProject && !selectingProject && !workLaneReady
      ? [
          'Open Agents.',
          `Choose this project: ${selectedProject.name}.`,
          'Create one task queue for new tasks.',
          'Come back here. Success looks like this card saying Task can be created.',
        ]
      : []
  const taskCapableAgents = agents.filter(agentHasTaskCapability)
  const assignableAgents = agents.filter(agentCanTakeTask)
  const hasOnlyNonTaskAgents = agents.length > 0 && taskCapableAgents.length === 0
  const taskWillWaitForAgent = workLaneReady && assignableAgents.length === 0
  const missingAgentDetail = agentSetupDetail({
    workLaneReady,
    hasProject: Boolean(selectedProject),
    hasAgents: agents.length > 0,
    hasTaskCapableAgents: taskCapableAgents.length > 0,
  })
  const projectGroups = useMemo(() => groupProjectsByTeam(projects), [projects])
  const projectField = register('projectId')
  const titleValue = watch('title')
  const descriptionValue = watch('description')
  const priorityValue = watch('priority')
  const assignedToValue = watch('assignedTo')
  const briefCues = useMemo(
    () => taskBriefCues(titleValue, descriptionValue),
    [descriptionValue, titleValue]
  )
  const missingBriefCues = useMemo(() => briefCues.filter((cue) => !cue.ready), [briefCues])
  const missingBriefCueLabel = formatBriefCueList(missingBriefCues.map((cue) => cue.label))
  const briefReady = missingBriefCues.length === 0
  const incompleteBriefActionLabel = taskWillWaitForAgent
    ? 'Save task anyway'
    : 'Create task anyway'
  const selectedAssignedAgent = agents.find((agent) => agent.id === assignedToValue)
  // Image upload is offered only when the assignee is a container CLI agent
  // running a vision-capable tool (claude/codex/gemini). Host CLI, opencode, and
  // Provider+Prompt/API assignees are excluded so the user never sees an
  // affordance that would fail at the server dispatch gate. Images upload scoped
  // to that agent's workspace; switching the assignee clears them since they
  // belong to the previous agent's workspace.
  const canAttachImages = isTaskImageCapable(selectedAssignedAgent)

  useEffect(() => {
    setImageIds([])
    setImagePreviews([])
    setImageError(null)
    assignedToRef.current = assignedToValue
  }, [assignedToValue])

  async function uploadFiles(files: File[]) {
    if (!canAttachImages || files.length === 0) return
    // The upload scopes the image to THIS assignee's workspace. If the user
    // switches/clears the assignee while it's in flight, the result belongs to
    // the old agent and must be discarded rather than submitted for the wrong one.
    const uploadAssignee = assignedToValue
    setImageError(null)
    setUploadingImage(true)
    try {
      for (const file of files) {
        if (!file.type.startsWith('image/')) {
          setImageError('Only image files can be attached.')
          continue
        }
        const res = await uploadImage(uploadAssignee, file)
        if (assignedToRef.current !== uploadAssignee) {
          // Assignee changed mid-upload — drop this now-stale attachment.
          continue
        }
        if (res.ok && res.id) {
          const id = res.id
          setImageIds((ids) => [...ids, id])
          setImagePreviews((prev) => [...prev, { id, name: file.name || 'image' }])
        } else {
          setImageError('Image upload failed. Check the file and try again.')
        }
      }
    } finally {
      setUploadingImage(false)
    }
  }

  function removeImage(id: string) {
    setImageIds((ids) => ids.filter((existing) => existing !== id))
    setImagePreviews((prev) => prev.filter((preview) => preview.id !== id))
  }

  const taskOptionsSummary = `${PRIORITY_LABELS[priorityValue]} priority, ${
    selectedAssignedAgent
      ? `${selectedAssignedAgent.name} starts first`
      : 'next ready agent starts it'
  }`
  const submitPreview = !selectedProject
    ? 'Choose a project first. Forge needs a home for this task and its history.'
    : !workLaneReady
      ? 'Set up a task queue first. Then this task will have a safe place to wait.'
      : taskWillWaitForAgent
        ? 'After you save, the task waits here until an agent is ready.'
        : 'After you create it, the next ready agent can start it from this project.'
  const TaskTemplateDisclosureIcon = taskTemplatesOpen ? ChevronDown : ChevronRight

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
      setConfirmIncompleteBrief(false)
      setTaskTemplatesOpen(true)
      setTaskOptionsOpen(false)
    }
  }, [isOpen])

  useEffect(() => {
    if (taskOptionsOpen && submitError === ASSIGNED_AGENT_NOT_READY_ERROR) {
      setFocus('assignedTo')
    }
  }, [setFocus, submitError, taskOptionsOpen])

  useEffect(() => {
    setConfirmIncompleteBrief(false)
  }, [titleValue, descriptionValue])

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
      setSubmitError(PROJECT_REQUIRED_ERROR)
      return
    }
    if (!selectedTaskGroupId) {
      setSubmitError(TASK_WAITING_PLACE_REQUIRED_ERROR)
      return
    }
    if (
      data.assignedTo &&
      !agents.some((agent) => agent.id === data.assignedTo && agentCanTakeTask(agent))
    ) {
      setSubmitError(ASSIGNED_AGENT_NOT_READY_ERROR)
      setTaskOptionsOpen(true)
      return
    }
    if (!briefReady && !confirmIncompleteBrief) {
      setConfirmIncompleteBrief(true)
      return
    }
    try {
      await onSubmit({ ...data, title: data.title.trim(), imageAttachmentIds: imageIds })
    } catch (err) {
      setSubmitError(boardActionErrorMessage('createTask', err))
      return
    }
    reset()
    onClose()
  }

  async function handleProjectChange(projectId: string) {
    setSubmitError(null)
    setConfirmIncompleteBrief(false)
    if (!projectId || !onProjectChange) return
    setSelectingProject(true)
    try {
      const ok = await onProjectChange(projectId)
      if (ok === false) {
        setSubmitError(
          'Select the project again to find the task queue. If it still does not load, open the Tasks page again or ask an owner to check the task queue in this project.'
        )
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
              Tell an agent what to do
            </h2>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Write the result you want. Forge will show whether the task has a project, a task
              queue, and enough detail before you save.
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
          <div className="mb-4 rounded-lg border border-apple-orange/20 bg-apple-orange/10 px-3 py-2 text-ui-caption text-apple-orange">
            <div className="flex gap-2">
              <AlertTriangle
                size={14}
                strokeWidth={2}
                className="mt-0.5 shrink-0"
                aria-hidden="true"
              />
              <div className="min-w-0 flex-1">
                <p className="font-semibold">Create a project before sending tasks</p>
                <p className="mt-0.5">
                  Projects keep each task, its files, and its activity history in one place.
                </p>
              </div>
            </div>
            {onOpenProjectSettings && (
              <button
                type="button"
                onClick={onOpenProjectSettings}
                className="mt-3 inline-flex h-8 items-center justify-center rounded-full border border-apple-orange/30 bg-white px-3 text-ui-button font-medium text-apple-orange transition-colors hover:bg-apple-orange/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-orange/35 dark:bg-white/[0.06]"
              >
                Open project settings
              </button>
            )}
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
          <div className="mb-4 rounded-lg border border-apple-orange/20 bg-apple-orange/10 px-3 py-2 text-ui-caption text-apple-orange">
            <div className="flex gap-2">
              <AlertTriangle
                size={14}
                strokeWidth={2}
                className="mt-0.5 shrink-0"
                aria-hidden="true"
              />
              <div className="min-w-0 flex-1">
                <p className="font-semibold">Connect an agent before this task can start</p>
                <p className="mt-0.5">{missingAgentDetail}</p>
              </div>
            </div>
            {onOpenAgentSetup && (
              <button
                type="button"
                onClick={onOpenAgentSetup}
                className="mt-3 inline-flex h-8 items-center justify-center rounded-full border border-apple-orange/30 bg-white px-3 text-ui-button font-medium text-apple-orange transition-colors hover:bg-apple-orange/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-orange/35 dark:bg-white/[0.06]"
              >
                Open Agents
              </button>
            )}
          </div>
        )}

        {agents.length > 0 && assignableAgents.length === 0 && (
          <div className="mb-4 rounded-lg border border-apple-orange/20 bg-apple-orange/10 px-3 py-2 text-ui-caption text-apple-orange">
            <div className="flex gap-2">
              <AlertTriangle
                size={14}
                strokeWidth={2}
                className="mt-0.5 shrink-0"
                aria-hidden="true"
              />
              <div className="min-w-0 flex-1">
                <p className="font-semibold">
                  {hasOnlyNonTaskAgents
                    ? 'Create a task-ready agent before this task can start'
                    : 'Start or connect an agent before this task can start'}
                </p>
                <p className="mt-0.5">{missingAgentDetail}</p>
              </div>
            </div>
            {onOpenAgentSetup && (
              <button
                type="button"
                onClick={onOpenAgentSetup}
                className="mt-3 inline-flex h-8 items-center justify-center rounded-full border border-apple-orange/30 bg-white px-3 text-ui-button font-medium text-apple-orange transition-colors hover:bg-apple-orange/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-orange/35 dark:bg-white/[0.06]"
              >
                Open Agents
              </button>
            )}
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
                <p className="font-semibold">{readinessTitle}</p>
                <p className="mt-0.5 text-secondary-light dark:text-secondary-dark">
                  {readinessDetail}
                </p>
                {waitingPlaceSetupSteps.length > 0 && (
                  <ol className="mt-2 list-decimal space-y-1 pl-4 text-secondary-light dark:text-secondary-dark">
                    {waitingPlaceSetupSteps.map((step) => (
                      <li key={step}>{step}</li>
                    ))}
                  </ol>
                )}
              </div>
            </div>
            {!workLaneReady && onOpenTaskRouting && (
              <button
                type="button"
                onClick={onOpenTaskRouting}
                className="mt-3 inline-flex h-8 items-center justify-center rounded-full border border-apple-orange/30 bg-white px-3 text-ui-button font-medium text-apple-orange transition-colors hover:bg-apple-orange/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-orange/35 dark:bg-white/[0.06]"
              >
                Set up task queue
              </button>
            )}
          </div>
        )}

        {submitError && (
          <div
            ref={errorBannerRef}
            role="alert"
            aria-live="polite"
            className="mb-4 rounded-lg bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
          >
            <div className="flex flex-wrap items-center gap-2">
              <span className="min-w-0 flex-1">{submitError}</span>
              {submitError === PROJECT_REQUIRED_ERROR && onOpenProjectSettings && (
                <button
                  type="button"
                  onClick={onOpenProjectSettings}
                  className="inline-flex h-7 shrink-0 items-center justify-center rounded-full border border-apple-red/20 bg-white/70 px-2.5 text-ui-button font-medium text-apple-red transition-colors hover:bg-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/35 dark:bg-white/[0.08] dark:hover:bg-white/[0.12]"
                >
                  Open project settings
                </button>
              )}
              {submitError === TASK_WAITING_PLACE_REQUIRED_ERROR && onOpenTaskRouting && (
                <button
                  type="button"
                  onClick={onOpenTaskRouting}
                  className="inline-flex h-7 shrink-0 items-center justify-center rounded-full border border-apple-red/20 bg-white/70 px-2.5 text-ui-button font-medium text-apple-red transition-colors hover:bg-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/35 dark:bg-white/[0.08] dark:hover:bg-white/[0.12]"
                >
                  Set up task queue
                </button>
              )}
            </div>
          </div>
        )}

        {confirmIncompleteBrief && missingBriefCues.length > 0 && (
          <div
            role="status"
            data-testid="task-brief-confirmation"
            className="mb-4 rounded-lg border border-apple-orange/20 bg-apple-orange/10 px-3 py-2 text-ui-caption text-apple-orange"
          >
            <p className="font-semibold">Add missing details before this task starts.</p>
            <p className="mt-0.5">
              Missing: {missingBriefCueLabel}. Best next step: add {missingBriefCueLabel}. If you
              choose {incompleteBriefActionLabel}, the agent may pause to ask follow-up questions.
            </p>
          </div>
        )}

        <form noValidate onSubmit={handleSubmit(handleFormSubmit)} className="flex flex-col gap-4">
          <div className="rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.04]">
            <button
              type="button"
              aria-expanded={taskTemplatesOpen}
              onClick={() => setTaskTemplatesOpen((open) => !open)}
              className="flex w-full items-center justify-between gap-3 text-left"
            >
              <span className="min-w-0">
                <span className="block text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
                  {taskTemplatesOpen ? 'Hide task writing help' : 'Need help writing the task?'}
                </span>
                <span className="mt-0.5 block text-ui-caption text-secondary-light dark:text-secondary-dark">
                  Use a starter template when you are not sure what to write.
                </span>
              </span>
              <TaskTemplateDisclosureIcon
                size={15}
                strokeWidth={2.2}
                className="shrink-0 text-apple-blue"
                aria-hidden="true"
              />
            </button>

            {taskTemplatesOpen && (
              <div className="mt-3">
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
                          : 'border-black/[0.08] bg-white text-foreground-light hover:bg-black/[0.04] dark:border-white/[0.1] dark:bg-black/20 dark:text-foreground-dark dark:hover:bg-white/[0.07]'
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
                <div className="mt-3 rounded-lg border border-black/[0.06] bg-white px-3 py-2.5 dark:border-white/[0.08] dark:bg-black/20">
                  <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    A clear task has three plain-language parts
                  </div>
                  <div className="mt-2 grid gap-1.5 sm:grid-cols-3">
                    {AGENT_READY_BRIEF_POINTS.map((point) => (
                      <div
                        key={point.label}
                        className="min-w-0 rounded-md bg-black/[0.025] px-2 py-1.5 dark:bg-white/[0.04]"
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
            )}
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
              <p
                id="task-title-error"
                role="alert"
                aria-live="polite"
                className="mt-1 text-ui-caption text-apple-red"
              >
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
            <div
              data-testid="task-brief-checklist"
              className="mt-2 rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.04]"
            >
              <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Make this task easy to start
              </p>
              <div className="mt-2 grid gap-1.5">
                {briefCues.map((cue) => (
                  <div
                    key={cue.id}
                    data-testid={`task-brief-cue-${cue.id}`}
                    className="flex items-start gap-2 rounded-md bg-white px-2 py-1.5 dark:bg-black/20"
                  >
                    <span
                      className={cn(
                        'mt-0.5 inline-flex h-5 min-w-12 items-center justify-center rounded-full px-2 text-[10px] font-semibold',
                        cue.ready
                          ? 'bg-apple-blue/10 text-apple-blue'
                          : 'bg-apple-orange/10 text-apple-orange'
                      )}
                    >
                      {cue.ready ? 'Ready' : 'Add'}
                    </span>
                    <span className="min-w-0">
                      <span className="block text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
                        {cue.label}
                      </span>
                      <span className="block text-ui-caption text-secondary-light dark:text-secondary-dark">
                        {cue.ready ? cue.readyDetail : cue.missingDetail}
                      </span>
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </div>

          <div className="rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.04]">
            <button
              type="button"
              aria-expanded={taskOptionsOpen}
              onClick={() => setTaskOptionsOpen((open) => !open)}
              className="flex w-full items-center justify-between gap-3 text-left"
            >
              <span>
                <span className="block text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
                  Task options
                </span>
                <span className="mt-0.5 block text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {taskOptionsSummary}
                </span>
              </span>
              <span className="shrink-0 text-ui-caption font-medium text-apple-blue">
                {taskOptionsOpen ? 'Hide' : 'Change'}
              </span>
            </button>

            {taskOptionsOpen && (
              <div className="mt-3 flex flex-col gap-4 sm:flex-row">
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
                    Normal is right for most work. Use Urgent only when people are waiting on it
                    now.
                  </p>
                </div>
                <div className="flex-1">
                  <div className="mb-1 flex items-center justify-between gap-2">
                    <label
                      htmlFor="task-assigned-to"
                      className="block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
                    >
                      Who should start it?
                    </label>
                    <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                      {assignableAgents.length} ready
                    </span>
                  </div>
                  <select
                    id="task-assigned-to"
                    {...register('assignedTo')}
                    className="h-10 w-full rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                  >
                    <option value="">Let the next ready agent start it</option>
                    {agents.map((a) => (
                      <option key={a.id} value={a.id} disabled={!agentCanTakeTask(a)}>
                        {a.name} ({agentTaskStatusLabel(a)})
                      </option>
                    ))}
                  </select>
                  <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {taskWillWaitForAgent
                      ? 'This task will wait here until an agent is ready.'
                      : 'Use the next ready agent when any ready agent can do the work.'}
                  </p>
                </div>
              </div>
            )}
          </div>

          <div
            data-testid="task-submit-preview"
            className="rounded-lg border border-apple-blue/20 bg-apple-blue/10 px-3 py-2.5 text-ui-caption text-foreground-light dark:text-foreground-dark"
          >
            <p className="font-semibold text-apple-blue">What happens after this</p>
            <p className="mt-1 text-secondary-light dark:text-secondary-dark">{submitPreview}</p>
          </div>

          {canAttachImages && (
            <div className="flex flex-col gap-2">
              <input
                ref={imageInputRef}
                type="file"
                accept="image/*"
                multiple
                className="hidden"
                onChange={(e) => {
                  void uploadFiles(Array.from(e.target.files ?? []))
                  e.target.value = ''
                }}
              />
              <div className="flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  onClick={() => imageInputRef.current?.click()}
                  disabled={uploadingImage}
                  className="inline-flex items-center gap-1.5 rounded-full border border-black/[0.08] px-3 py-1.5 text-ui-caption text-secondary-light hover:bg-black/[0.03] disabled:opacity-50 dark:border-white/[0.1] dark:text-secondary-dark"
                >
                  <ImagePlus className="size-4" aria-hidden />
                  {uploadingImage ? 'Uploading…' : 'Attach image'}
                </button>
                <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                  Add a screenshot for a vision-capable agent (e.g. Claude Code, Codex).
                </span>
              </div>
              {imagePreviews.length > 0 && (
                <ul className="flex flex-wrap gap-2">
                  {imagePreviews.map((preview) => (
                    <li
                      key={preview.id}
                      className="inline-flex items-center gap-1.5 rounded-full bg-black/[0.05] px-2.5 py-1 text-ui-caption text-foreground-light dark:bg-white/[0.08] dark:text-foreground-dark"
                    >
                      <ImagePlus className="size-3.5" aria-hidden />
                      <span className="max-w-[140px] truncate">{preview.name}</span>
                      <button
                        type="button"
                        onClick={() => removeImage(preview.id)}
                        aria-label={`Remove ${preview.name}`}
                        className="text-secondary-light hover:text-apple-red dark:text-secondary-dark"
                      >
                        <X className="size-3.5" aria-hidden />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {imageError && (
                <div className="text-ui-caption text-apple-red" role="alert" aria-live="polite">
                  {imageError}
                </div>
              )}
            </div>
          )}

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
              disabled={isSubmitting || selectingProject || uploadingImage}
              aria-busy={isSubmitting || selectingProject || uploadingImage}
              className="w-full rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95 disabled:cursor-not-allowed disabled:opacity-60 sm:w-auto"
            >
              {uploadingImage
                ? 'Uploading image...'
                : selectingProject
                  ? 'Preparing project...'
                  : isSubmitting
                    ? taskWillWaitForAgent
                      ? 'Saving task to wait...'
                      : 'Creating task...'
                    : confirmIncompleteBrief && !briefReady
                      ? incompleteBriefActionLabel
                      : taskWillWaitForAgent
                        ? 'Save task to wait'
                        : 'Create task'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

function agentSetupDetail({
  workLaneReady,
  hasProject,
  hasAgents,
  hasTaskCapableAgents,
}: {
  workLaneReady: boolean
  hasProject: boolean
  hasAgents: boolean
  hasTaskCapableAgents: boolean
}): string {
  if (hasAgents && !hasTaskCapableAgents) {
    if (workLaneReady) {
      return 'Simple chat agents answer questions in Chat. For Tasks, open Agents and create or start a Project files or This computer agent.'
    }
    return 'Set up a task queue first. For Tasks, use a Project files or This computer agent instead of Simple chat.'
  }

  if (workLaneReady) {
    return hasAgents
      ? 'Save the task now. It will wait here until one of your agents is ready. To start it sooner, open Agents.'
      : 'Save the task now. It will wait here until an agent is ready. To start it sooner, open Agents.'
  }

  const setupStep = hasProject
    ? 'Set up a task queue first.'
    : 'Create a project and set up a task queue first.'
  const waitTarget = hasAgents ? 'one of your agents' : 'an agent'
  return `${setupStep} Then this task can wait here until ${waitTarget} is ready. To fix agent setup now, open Agents.`
}

function taskBriefCues(title: string, description: string): TaskBriefCue[] {
  const normalizedTitle = title.trim()
  const hasPersonalizedTitle = normalizedTitle.length > 0 && !isTemplateTaskTitle(normalizedTitle)
  const contentText = meaningfulBriefText(description)
  const hasWorkSectionContent = hasBriefSectionContent(description, [
    'where to work',
    'where to look first',
    'what to inspect',
    'what to avoid',
    'change to check',
  ])
  const hasDoneSectionContent = hasBriefSectionContent(description, [
    'done when',
    'checks to run',
    'what to check',
    'answer format',
    'answer needed',
    'decision needed',
  ])
  const namesWorkArea =
    hasWorkSectionContent ||
    /\b(files?|folder|screen|page|area|src\/|docs\/|tests?\/|rust\/)\b/.test(contentText)
  const namesFinishCheck =
    hasDoneSectionContent ||
    /\b(success|verify|test|check|screenshot|output|result|passes?)\b/.test(contentText)

  return [
    {
      id: 'goal',
      label: 'Result',
      ready: hasPersonalizedTitle,
      readyDetail: 'The agent has a clear result to finish.',
      missingDetail:
        normalizedTitle.length > 0
          ? 'Replace the template title with the specific result you want.'
          : 'Write one sentence for the result you want.',
    },
    {
      id: 'where',
      label: 'Where to work',
      ready: namesWorkArea,
      readyDetail: 'The agent knows where to look or what to avoid.',
      missingDetail: 'Name the page, screen, file, or area to check first.',
    },
    {
      id: 'done',
      label: 'Done when',
      ready: namesFinishCheck,
      readyDetail: 'The agent knows how success will be checked.',
      missingDetail: 'Add the simple check, screenshot, or result that proves it is done.',
    },
  ]
}

function isTemplateTaskTitle(title: string): boolean {
  const normalizedTitle = title.trim().toLowerCase()
  return TASK_BRIEF_TEMPLATES.some((template) => template.title.toLowerCase() === normalizedTitle)
}

function meaningfulBriefText(description: string): string {
  return description
    .split(/\r?\n/)
    .map(cleanBriefLine)
    .filter((line) => line.length > 0 && !isTemplateCueLabel(line) && !isTemplateHelperLine(line))
    .join('\n')
    .toLowerCase()
}

function hasBriefSectionContent(description: string, labels: string[]): boolean {
  const normalizedLabels = new Set(labels.map((label) => label.toLowerCase()))
  let inSection = false

  for (const rawLine of description.split(/\r?\n/)) {
    const line = cleanBriefLine(rawLine)
    if (!line) continue

    const cueLabel = templateCueLabel(line)
    if (cueLabel) {
      inSection = normalizedLabels.has(cueLabel)
      continue
    }

    if (isTemplateHelperLine(line)) continue

    if (inSection) return true
  }

  return false
}

function cleanBriefLine(line: string): string {
  return line.replace(/^[-*]\s*/, '').trim()
}

function templateCueLabel(line: string): string | null {
  const normalized = line.trim().replace(/:$/, '').toLowerCase()
  return isTemplateCueLabel(normalized) ? normalized : null
}

function isTemplateCueLabel(line: string): boolean {
  return TEMPLATE_CUE_LABELS.has(line.trim().replace(/:$/, '').toLowerCase())
}

function isTemplateHelperLine(line: string): boolean {
  return TEMPLATE_HELPER_LINES.has(line.trim().toLowerCase())
}

const TEMPLATE_CUE_LABELS = new Set([
  'what should change',
  'where to work',
  'what to avoid',
  'done when',
  'what is broken',
  'what should happen',
  'where to look first',
  'question to answer',
  'what to inspect',
  'what is already known',
  'decision needed',
  'change to check',
  'what could go wrong',
  'checks to run',
  'what to check',
  'answer format',
  'answer needed',
])

const TEMPLATE_HELPER_LINES = new Set([
  'describe what you want to see or use after this is done.',
  'name the page or area if you know it.',
  'list anything that should stay unchanged.',
  'say what should be visible, ready to use, or easy to check.',
  'describe what you see now.',
  'describe the correct result.',
  'name the page or step where you saw it.',
  'say how you will know the problem is fixed.',
  'write the question in one sentence.',
  'add pages, clues, links, screenshots, or recent changes if you know them.',
  'add what you already tried or noticed.',
  'say what answer or recommendation you need.',
  'name what changed and where a user would see it.',
  'list the risks you care about.',
  'say what you want the agent to check, such as a screen, result, or sign-in step.',
  'ask for what is safe, what needs fixing, and what to do next.',
])

function formatBriefCueList(labels: string[]): string {
  if (labels.length === 0) return 'the missing details'
  if (labels.length === 1) return labels[0].toLowerCase()
  return `${labels.slice(0, -1).join(', ').toLowerCase()} and ${labels[labels.length - 1].toLowerCase()}`
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
