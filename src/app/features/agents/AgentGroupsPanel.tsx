import { type FormEvent, useEffect, useId, useMemo, useRef, useState } from 'react'
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Check,
  ClipboardCheck,
  Clock3,
  CircleDot,
  Layers3,
  Plus,
  Search,
  ShieldCheck,
  Wrench,
  X,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { waitingPlaceDisplayName } from '@app/entities/navigation/agent-group'
import { useNavigationStore } from '@app/entities/navigation'
import { agentGroupErrorMessage } from './model/agentGroupErrorMessage'

const DEFAULT_GROUP_DESCRIPTION = 'Project tasks wait here until an available agent starts them.'
const ROUTING_SEARCH_HELP =
  'Search only looks inside this place. Use Show all tasks here to return to every task waiting here.'

const TASK_STATE_LABELS: Record<TaskSummary['state'], string> = {
  backlog: 'Not sent yet',
  queued: 'Waiting to start',
  working: 'Working',
  blocked: 'Needs help',
  completed: 'Done',
  failed: 'Check retry steps',
  canceled: 'Canceled',
}

const TASK_STATE_DOT: Record<TaskSummary['state'], string> = {
  backlog: 'bg-apple-gray-2',
  queued: 'bg-apple-blue',
  working: 'bg-apple-green',
  blocked: 'bg-apple-orange',
  completed: 'bg-apple-green',
  failed: 'bg-apple-red',
  canceled: 'bg-apple-gray-2',
}

const STATE_SORT_WEIGHT: Record<TaskSummary['state'], number> = {
  blocked: 0,
  failed: 1,
  working: 2,
  queued: 3,
  backlog: 4,
  completed: 5,
  canceled: 6,
}

const PRIORITY_SORT_WEIGHT: Record<TaskSummary['priority'], number> = {
  urgent: 0,
  high: 1,
  normal: 2,
  low: 3,
}

interface TaskGroupTemplate {
  id: string
  label: string
  summary: string
  name: string
  description: string
  Icon: LucideIcon
}

const TASK_GROUP_TEMPLATES: TaskGroupTemplate[] = [
  {
    id: 'delivery',
    label: 'Delivery',
    summary: 'Build and verify',
    name: 'Delivery Tasks',
    description:
      'Build the requested changes, keep work moving, and run checks before sharing results.',
    Icon: Wrench,
  },
  {
    id: 'result-check',
    label: 'Check results',
    summary: 'Check before use',
    name: 'Result Check Tasks',
    description:
      'Check finished work for confusing behavior, missing checks, and anything that could make it unsafe to use.',
    Icon: ShieldCheck,
  },
  {
    id: 'sort-work',
    label: 'Sort work',
    summary: 'Clarify and send',
    name: 'Intake Tasks',
    description: 'Clarify incoming work, find what is missing, and send tasks to the right agent.',
    Icon: ClipboardCheck,
  },
]

interface AgentGroupsPanelProps {
  onOpenProjectsSetup?: () => void
}

export function AgentGroupsPanel({ onOpenProjectsSetup }: AgentGroupsPanelProps = {}) {
  const selectedProjectId = useNavigationStore((state) => state.selectedProjectId)
  const projectsByTeam = useNavigationStore((state) => state.projects)
  const agentGroups = useNavigationStore((state) => state.agentGroups)
  const createAgentGroup = useNavigationStore((state) => state.createAgentGroup)
  const columns = useBoardStore((state) => state.columns)
  const selectedGroupId = useBoardStore((state) => state.selectedGroupId)
  const setSelectedGroupId = useBoardStore((state) => state.setSelectedGroupId)
  const [formOpen, setFormOpen] = useState(false)
  const [name, setName] = useState('')
  const [description, setDescription] = useState(DEFAULT_GROUP_DESCRIPTION)
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(null)
  const [routingSearch, setRoutingSearch] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const nameInputRef = useRef<HTMLInputElement>(null)
  const routingSearchHelpId = useId()

  const selectedProject = useMemo(() => {
    if (!selectedProjectId) return null
    return (
      Object.values(projectsByTeam)
        .flat()
        .find((project) => project.id === selectedProjectId) ?? null
    )
  }, [projectsByTeam, selectedProjectId])

  const selectedGroup = useMemo(
    () => agentGroups.find((group) => group.id === selectedGroupId) ?? null,
    [agentGroups, selectedGroupId]
  )

  const groupTasks = useMemo(() => {
    if (!selectedGroupId) return []
    return Object.values(columns)
      .flat()
      .filter((task) => !task.groupId || task.groupId === selectedGroupId)
  }, [columns, selectedGroupId])

  const workload = useMemo(() => summarizeGroupTasks(groupTasks), [groupTasks])

  const visibleTasks = useMemo(
    () => filterAndSortGroupTasks(groupTasks, routingSearch).slice(0, 5),
    [groupTasks, routingSearch]
  )

  const hasRoutingSearch = routingSearch.trim().length > 0

  useEffect(() => {
    setName('')
    setDescription(DEFAULT_GROUP_DESCRIPTION)
    setSelectedTemplateId(null)
    setRoutingSearch('')
    setError(null)
  }, [selectedProjectId])

  useEffect(() => {
    setRoutingSearch('')
  }, [selectedGroupId])

  useEffect(() => {
    if (!selectedProjectId) {
      setFormOpen(false)
      setError(null)
      return
    }
    if (agentGroups.length === 0) setFormOpen(true)
  }, [agentGroups.length, selectedProjectId])

  async function handleCreateGroup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!selectedProjectId) {
      setError(
        'Open project settings to create a project, or choose an existing project before setting up a place for new tasks.'
      )
      return
    }

    const trimmedName = name.trim()
    if (!trimmedName) {
      setError('Name this place before creating it. Examples: Intake, Result Check, or Delivery.')
      nameInputRef.current?.focus()
      return
    }

    setSaving(true)
    setError(null)
    try {
      await createAgentGroup(selectedProjectId, {
        name: trimmedName,
        description: description.trim() || DEFAULT_GROUP_DESCRIPTION,
      })
      setName('')
      setDescription(DEFAULT_GROUP_DESCRIPTION)
      setSelectedTemplateId(null)
      setFormOpen(false)
    } catch (err) {
      setError(agentGroupErrorMessage(err))
    } finally {
      setSaving(false)
    }
  }

  function applyTemplate(template: TaskGroupTemplate) {
    setSelectedTemplateId(template.id)
    setName(template.name)
    setDescription(template.description)
    setError(null)
  }

  return (
    <section data-testid="agent-groups-panel" className={cn(uiStyles.cardPadded, 'p-6')}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Layers3
              size={15}
              strokeWidth={2}
              className="text-secondary-light dark:text-secondary-dark"
              aria-hidden="true"
            />
            <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              Places for new tasks
            </h2>
          </div>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Choose a place for new tasks before an agent starts them. Set up one place, add agents,
            then send tasks there.
          </p>
          {selectedProject && (
            <p className="mt-2 truncate rounded-md bg-black/[0.04] px-2 py-1 text-ui-caption text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
              {selectedProject.name}
            </p>
          )}
        </div>

        {selectedProjectId && !formOpen && (
          <button
            type="button"
            onClick={() => {
              setFormOpen(true)
              setError(null)
            }}
            className={cn(uiStyles.primaryButton, 'shrink-0')}
          >
            <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
            Set up place
          </button>
        )}
      </div>

      {!selectedProjectId ? (
        <div className="mt-3 rounded-card border border-dashed border-black/10 px-3 py-3 text-ui-caption text-secondary-light dark:border-white/10 dark:text-secondary-dark">
          <p>
            Choose a project first. If you do not have one, open project settings to create it.
            Projects keep related tasks, agents, and places together.
          </p>
          {onOpenProjectsSetup ? (
            <button
              type="button"
              onClick={onOpenProjectsSetup}
              className={cn(uiStyles.secondaryButton, 'mt-3')}
            >
              <span>Open project settings</span>
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </button>
          ) : null}
        </div>
      ) : (
        <div className="mt-3 flex flex-col gap-3">
          <div className="flex flex-wrap gap-2">
            {agentGroups.length > 0 ? (
              agentGroups.map((group) => {
                const isSelected = selectedGroupId === group.id
                return (
                  <button
                    key={group.id}
                    type="button"
                    aria-pressed={isSelected}
                    onClick={() => setSelectedGroupId(group.id)}
                    className={cn(
                      'inline-flex h-9 max-w-full items-center gap-1.5 rounded-button border px-4 text-ui-button font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                      isSelected
                        ? 'border-black/[0.08] bg-black/[0.06] text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.08] dark:text-foreground-dark'
                        : 'border-black/[0.08] bg-white text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
                    )}
                  >
                    {isSelected && <Check size={13} strokeWidth={2.25} aria-hidden="true" />}
                    <span className="truncate">{waitingPlaceDisplayName(group.name)}</span>
                  </button>
                )
              })
            ) : (
              <div className="rounded-card border border-dashed border-black/10 px-3 py-2 text-ui-caption text-secondary-light dark:border-white/10 dark:text-secondary-dark">
                Set up the first place so new tasks have somewhere to wait.
              </div>
            )}
          </div>

          {agentGroups.length > 0 && (
            <section
              data-testid="task-routing-workload"
              className="rounded-card border border-black/[0.08] bg-black/[0.02] p-3 dark:border-white/[0.1] dark:bg-white/[0.04]"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    Tasks waiting here
                  </p>
                  <h3 className="truncate text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                    {selectedGroup ? waitingPlaceDisplayName(selectedGroup.name) : 'Select a place'}
                  </h3>
                </div>
                <span className={cn(uiStyles.badge, 'shrink-0 bg-white dark:bg-black/20')}>
                  {workload.total} {workload.total === 1 ? 'task here' : 'tasks here'}
                </span>
              </div>

              <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
                <RoutingMetric
                  testId="routing-metric-active"
                  label="Working now"
                  value={workload.active}
                  Icon={CircleDot}
                  tone="active"
                />
                <RoutingMetric
                  testId="routing-metric-backlog"
                  label="Not sent yet"
                  value={workload.backlog}
                  Icon={Clock3}
                  tone="neutral"
                />
                <RoutingMetric
                  testId="routing-metric-needs-action"
                  label="Needs help"
                  value={workload.needsAction}
                  Icon={AlertTriangle}
                  tone="warn"
                />
                <RoutingMetric
                  testId="routing-metric-completed"
                  label="Done"
                  value={workload.completed}
                  Icon={CheckCircle2}
                  tone="success"
                />
              </div>

              <label className="relative mt-3 block">
                <span className="sr-only">Search tasks waiting here</span>
                <Search
                  size={14}
                  strokeWidth={2}
                  className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
                  aria-hidden="true"
                />
                <input
                  data-testid="task-routing-search"
                  type="search"
                  value={routingSearch}
                  onChange={(event) => setRoutingSearch(event.target.value)}
                  className={cn(uiStyles.input, 'h-9 pl-8 pr-3')}
                  placeholder="Search tasks, agents, or problems..."
                  aria-describedby={routingSearchHelpId}
                />
              </label>
              <p
                id={routingSearchHelpId}
                className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
              >
                {ROUTING_SEARCH_HELP}
              </p>

              <div className="mt-3">
                {groupTasks.length === 0 ? (
                  <p
                    data-testid="task-routing-empty"
                    className="rounded-card border border-dashed border-black/10 px-3 py-3 text-ui-caption text-secondary-light dark:border-white/10 dark:text-secondary-dark"
                  >
                    Create the first task for this place, then choose this place so the task waits
                    here.
                    <span className="mt-1 block">
                      Success looks like a task showing Waiting to start or Working here.
                    </span>
                  </p>
                ) : visibleTasks.length > 0 ? (
                  <ul className="flex flex-col gap-1.5">
                    {visibleTasks.map((task) => (
                      <RoutedTaskRow key={task.id} task={task} />
                    ))}
                  </ul>
                ) : (
                  <div
                    data-testid="task-routing-filter-empty"
                    role="status"
                    aria-live="polite"
                    className="flex flex-col gap-2 rounded-card border border-dashed border-black/10 px-3 py-3 text-ui-caption text-secondary-light dark:border-white/10 dark:text-secondary-dark"
                  >
                    <div className="space-y-1">
                      <p className="font-medium text-foreground-light dark:text-foreground-dark">
                        Search is hiding tasks here
                      </p>
                      <p>This place still has tasks, but none match the words you typed.</p>
                      <p>Next: show all tasks here before assuming this place is empty.</p>
                    </div>
                    {hasRoutingSearch && (
                      <button
                        type="button"
                        onClick={() => setRoutingSearch('')}
                        className={cn(uiStyles.secondaryButton, 'self-start')}
                      >
                        Show all tasks here
                      </button>
                    )}
                  </div>
                )}
              </div>
            </section>
          )}

          {formOpen && (
            <form onSubmit={handleCreateGroup} className="grid gap-2">
              <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                Choose an example below, or type your own place name. New tasks can wait there after
                you create it.
              </p>
              <div role="group" aria-label="Place examples" className="grid gap-2 sm:grid-cols-3">
                {TASK_GROUP_TEMPLATES.map((template) => (
                  <button
                    key={template.id}
                    type="button"
                    onClick={() => applyTemplate(template)}
                    aria-pressed={selectedTemplateId === template.id}
                    className={cn(
                      'flex min-h-16 min-w-0 items-center gap-2 rounded-card border px-2.5 py-2 text-left transition-colors',
                      selectedTemplateId === template.id
                        ? 'border-black/[0.08] bg-black/[0.06] text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.08] dark:text-foreground-dark'
                        : 'border-black/[0.08] bg-black/[0.02] text-foreground-light hover:bg-black/[0.04] dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.07]'
                    )}
                  >
                    <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-white text-apple-blue dark:bg-black/20">
                      <template.Icon size={15} strokeWidth={2.25} aria-hidden="true" />
                    </span>
                    <span className="min-w-0">
                      <span className="block truncate text-ui-button font-semibold">
                        {template.label}
                      </span>
                      <span className="block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                        {template.summary}
                      </span>
                    </span>
                  </button>
                ))}
              </div>

              <input
                ref={nameInputRef}
                aria-label="Place name"
                name="taskGroupName"
                autoComplete="off"
                value={name}
                aria-invalid={error && !name.trim() ? 'true' : undefined}
                onChange={(event) => {
                  setName(event.target.value)
                  if (error) setError(null)
                }}
                className={cn(uiStyles.input, 'h-10 px-4')}
                placeholder="Place name..."
                disabled={saving}
              />
              <input
                aria-label="Place description"
                name="taskGroupDescription"
                autoComplete="off"
                value={description}
                onChange={(event) => setDescription(event.target.value)}
                className={cn(uiStyles.input, 'h-10 px-4')}
                placeholder="What work should wait here?"
                disabled={saving}
              />
              <div className="flex items-center justify-end gap-2">
                <button
                  type="submit"
                  disabled={saving}
                  className={cn(uiStyles.primaryButton, 'h-10 px-4')}
                >
                  <Check size={14} strokeWidth={2.25} aria-hidden="true" />
                  {saving ? 'Creating...' : 'Create place'}
                </button>
                {agentGroups.length > 0 && (
                  <button
                    type="button"
                    onClick={() => {
                      setFormOpen(false)
                      setError(null)
                    }}
                    disabled={saving}
                    aria-label="Cancel place creation"
                    className={cn(uiStyles.subtleButton, 'h-10 w-10 px-0')}
                  >
                    <X size={14} strokeWidth={2.25} aria-hidden="true" />
                  </button>
                )}
              </div>
            </form>
          )}

          {error && (
            <p role="alert" aria-live="polite" className="text-ui-caption text-apple-red">
              {error}
            </p>
          )}
        </div>
      )}
    </section>
  )
}

function RoutingMetric({
  testId,
  label,
  value,
  Icon,
  tone,
}: {
  testId: string
  label: string
  value: number
  Icon: LucideIcon
  tone: 'active' | 'neutral' | 'success' | 'warn'
}) {
  const toneClass =
    tone === 'active'
      ? 'bg-apple-blue/10 text-apple-blue'
      : tone === 'success'
        ? 'bg-apple-green/10 text-apple-green'
        : tone === 'warn'
          ? 'bg-apple-orange/10 text-apple-orange'
          : 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.07] dark:text-secondary-dark'

  return (
    <div
      data-testid={testId}
      className="flex min-h-16 items-center gap-2 rounded-card border border-black/[0.06] bg-white px-2.5 py-2 dark:border-white/[0.08] dark:bg-black/20"
    >
      <span
        className={cn('flex h-8 w-8 shrink-0 items-center justify-center rounded-card', toneClass)}
      >
        <Icon size={15} strokeWidth={2.2} aria-hidden="true" />
      </span>
      <span className="min-w-0">
        <span className="block text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
          {value}
        </span>
        <span className="block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {label}
        </span>
      </span>
    </div>
  )
}

function RoutedTaskRow({ task }: { task: TaskSummary }) {
  const title = routedTaskTitle(task)
  const assignment = routedTaskAssignment(task)
  const nextStep = routedTaskNextStep(task)

  return (
    <li
      data-testid="task-routing-row"
      className="flex items-center justify-between gap-3 rounded-card border border-black/[0.06] bg-white px-3 py-2 dark:border-white/[0.08] dark:bg-black/20"
    >
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <span className="inline-flex shrink-0 items-center gap-1.5 text-ui-caption font-medium uppercase text-secondary-light dark:text-secondary-dark">
            <span className={cn('h-1.5 w-1.5 rounded-full', TASK_STATE_DOT[task.state])} />
            {TASK_STATE_LABELS[task.state]}
          </span>
          <p className="truncate text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            {title}
          </p>
        </div>
        <p className="mt-1 truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {assignment} · {nextStep}
        </p>
      </div>
      <span className="shrink-0 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
        {Math.round(task.progress)}%
      </span>
    </li>
  )
}

function routedTaskTitle(task: TaskSummary): string {
  const rawTitle = task.params.task.trim()
  if (!rawTitle) return 'Untitled task'

  return rawTitle.replace(/\brepository access\b/gi, (match) =>
    match[0] === 'R' ? 'Code access' : 'code access'
  )
}

function routedTaskAssignment(task: TaskSummary): string {
  if (task.assignedAgentName) return task.assignedAgentName
  if (task.assignedTo) return 'Chosen agent'
  return 'Needs agent'
}

function routedTaskNextStep(task: TaskSummary): string {
  switch (task.state) {
    case 'backlog':
      return task.assignedTo || task.assignedAgentName
        ? 'Ready to send'
        : 'Choose an agent before sending it'
    case 'queued':
      return 'Waiting for an available agent to start it'
    case 'working':
      return 'Watch live progress'
    case 'blocked':
      return taskBlockedPreview({
        blockedHint: task.blockedHint,
        blockedReason: task.blockedReason,
        error: task.error,
      })
    case 'failed':
      return taskFailurePreview(task.error)
    case 'completed':
      return 'Check the finished result'
    case 'canceled':
      return 'Stopped before completion'
  }
}

function summarizeGroupTasks(tasks: TaskSummary[]): {
  total: number
  active: number
  backlog: number
  needsAction: number
  completed: number
} {
  return tasks.reduce(
    (summary, task) => {
      summary.total += 1
      if (task.state === 'queued' || task.state === 'working') summary.active += 1
      if (task.state === 'backlog') summary.backlog += 1
      if (task.state === 'blocked' || task.state === 'failed') summary.needsAction += 1
      if (task.state === 'completed') summary.completed += 1
      return summary
    },
    { total: 0, active: 0, backlog: 0, needsAction: 0, completed: 0 }
  )
}

function filterAndSortGroupTasks(tasks: TaskSummary[], query: string): TaskSummary[] {
  const normalizedQuery = query.trim().toLowerCase()
  const filtered =
    normalizedQuery.length === 0
      ? tasks
      : tasks.filter((task) => groupTaskSearchText(task).includes(normalizedQuery))

  return [...filtered].sort((a, b) => {
    const stateDelta = STATE_SORT_WEIGHT[a.state] - STATE_SORT_WEIGHT[b.state]
    if (stateDelta !== 0) return stateDelta

    const priorityDelta = PRIORITY_SORT_WEIGHT[a.priority] - PRIORITY_SORT_WEIGHT[b.priority]
    if (priorityDelta !== 0) return priorityDelta

    return new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()
  })
}

function groupTaskSearchText(task: TaskSummary): string {
  return [
    routedTaskTitle(task),
    TASK_STATE_LABELS[task.state],
    routedTaskAssignment(task),
    routedTaskNextStep(task),
    task.assignedAgentName,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
}
