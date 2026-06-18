import { type FormEvent, useEffect, useMemo, useRef, useState } from 'react'
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
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { useBoardStore } from '@app/shared/model/board.store'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { useNavigationStore } from '@app/entities/navigation'
import { agentGroupErrorMessage } from './model/agentGroupErrorMessage'

const DEFAULT_GROUP_DESCRIPTION = 'Project tasks wait here until an available agent picks them up.'

const TASK_STATE_LABELS: Record<TaskSummary['state'], string> = {
  backlog: 'Not sent yet',
  queued: 'Waiting to start',
  working: 'Working',
  blocked: 'Needs help',
  completed: 'Done',
  failed: 'Review recovery',
  canceled: 'Canceled',
}

const TASK_STATE_TONE: Record<TaskSummary['state'], string> = {
  backlog: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.07] dark:text-secondary-dark',
  queued: 'bg-apple-blue/10 text-apple-blue',
  working: 'bg-apple-green/10 text-apple-green',
  blocked: 'bg-apple-orange/10 text-apple-orange',
  completed: 'bg-apple-green/10 text-apple-green',
  failed: 'bg-apple-red/10 text-apple-red',
  canceled: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.07] dark:text-secondary-dark',
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
    id: 'review',
    label: 'Review',
    summary: 'Check before release',
    name: 'Review Tasks',
    description:
      'Review completed work for broken behavior, missing tests, and anything that could block release.',
    Icon: ShieldCheck,
  },
  {
    id: 'triage',
    label: 'Sort work',
    summary: 'Clarify and assign',
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
        'Open project settings to create a project, or choose an existing project before setting up where tasks wait.'
      )
      return
    }

    const trimmedName = name.trim()
    if (!trimmedName) {
      setError('Name this waiting place before creating it. Examples: Intake, Review, or Delivery.')
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
    <section
      data-testid="agent-groups-panel"
      className="rounded-card border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
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
              Where Tasks Wait
            </h2>
          </div>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            These shared waiting places tell agents where to pick up new work. Set up one place, add
            agents, then send tasks there.
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
            className={cn(
              'inline-flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-lg px-2.5 text-ui-button font-medium transition-colors',
              'rounded-full bg-apple-blue text-white hover:bg-apple-blue-focus',
              'transition-transform active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus'
            )}
          >
            <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
            Set up waiting place
          </button>
        )}
      </div>

      {!selectedProjectId ? (
        <div className="mt-3 rounded-lg border border-dashed border-black/10 px-3 py-3 text-ui-caption text-secondary-light dark:border-white/10 dark:text-secondary-dark">
          <p>
            Open project settings to create a project, or choose an existing project from the
            project list. Each project keeps its own waiting places and agents.
          </p>
          {onOpenProjectsSetup ? (
            <button
              type="button"
              onClick={onOpenProjectsSetup}
              className="mt-3 inline-flex h-8 items-center justify-center gap-1.5 rounded-full border border-apple-blue/20 bg-apple-blue/[0.08] px-3 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/[0.12] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35"
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
                      'inline-flex h-9 max-w-full items-center gap-1.5 rounded-full border px-4 text-ui-button font-medium transition-transform active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                      isSelected
                        ? 'border-apple-blue-focus bg-white text-foreground-light shadow-[inset_0_0_0_1px_#0071e3] dark:bg-white/[0.04] dark:text-foreground-dark'
                        : 'border-black/[0.08] bg-white text-foreground-light hover:border-black/20 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:border-white/20'
                    )}
                  >
                    {isSelected && <Check size={13} strokeWidth={2.25} aria-hidden="true" />}
                    <span className="truncate">{group.name}</span>
                  </button>
                )
              })
            ) : (
              <div className="rounded-lg border border-dashed border-black/10 px-3 py-2 text-ui-caption text-secondary-light dark:border-white/10 dark:text-secondary-dark">
                Set up the first waiting place so agents know where to pick up tasks.
              </div>
            )}
          </div>

          {agentGroups.length > 0 && (
            <section
              data-testid="task-routing-workload"
              className="rounded-xl border border-black/[0.08] bg-black/[0.02] p-3 dark:border-white/[0.1] dark:bg-white/[0.04]"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                    Tasks waiting here
                  </p>
                  <h3 className="truncate text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                    {selectedGroup?.name ?? 'Select where tasks wait'}
                  </h3>
                </div>
                <span className="shrink-0 rounded-full bg-white px-2 py-1 text-ui-caption font-medium text-secondary-light shadow-sm dark:bg-black/20 dark:text-secondary-dark">
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
                <span className="sr-only">Search tasks in this waiting place</span>
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
                  className="h-9 w-full rounded-lg border border-black/[0.08] bg-white pl-8 pr-3 text-ui-body text-foreground-light outline-none placeholder:text-secondary-light focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-[#2a2a2c] dark:text-foreground-dark dark:placeholder:text-secondary-dark"
                  placeholder="Search tasks, agents, or problems..."
                />
              </label>

              <div className="mt-3">
                {groupTasks.length === 0 ? (
                  <p
                    data-testid="task-routing-empty"
                    className="rounded-lg border border-dashed border-black/10 px-3 py-3 text-ui-caption text-secondary-light dark:border-white/10 dark:text-secondary-dark"
                  >
                    Create the first task for this waiting place, then choose it so agents know
                    where to pick up the task.
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
                    className="flex flex-col gap-2 rounded-lg border border-dashed border-black/10 px-3 py-3 text-ui-caption text-secondary-light dark:border-white/10 dark:text-secondary-dark"
                  >
                    <div className="space-y-1">
                      <p className="font-medium text-foreground-light dark:text-foreground-dark">
                        Search is hiding tasks in this waiting place
                      </p>
                      <p>This waiting place still has tasks, but none match the words you typed.</p>
                      <p>Next: show all tasks here before assuming this place is empty.</p>
                    </div>
                    {hasRoutingSearch && (
                      <button
                        type="button"
                        onClick={() => setRoutingSearch('')}
                        className="self-start rounded-full bg-white px-2.5 py-1 text-ui-button font-medium text-apple-blue shadow-sm transition-colors hover:bg-apple-blue/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:bg-black/20"
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
                Choose what kind of work should wait here, or name a place yourself. New tasks can
                use it as soon as the place is ready.
              </p>
              <div
                role="group"
                aria-label="Waiting place templates"
                className="grid gap-2 sm:grid-cols-3"
              >
                {TASK_GROUP_TEMPLATES.map((template) => (
                  <button
                    key={template.id}
                    type="button"
                    onClick={() => applyTemplate(template)}
                    aria-pressed={selectedTemplateId === template.id}
                    className={cn(
                      'flex min-h-16 min-w-0 items-center gap-2 rounded-lg border px-2.5 py-2 text-left transition-colors',
                      selectedTemplateId === template.id
                        ? 'border-apple-blue/40 bg-apple-blue/10 text-foreground-light dark:text-foreground-dark'
                        : 'border-black/[0.08] bg-black/[0.02] text-foreground-light hover:bg-black/[0.04] dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.07]'
                    )}
                  >
                    <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-white text-apple-blue shadow-sm dark:bg-black/20">
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
                aria-label="Waiting place name"
                name="taskGroupName"
                autoComplete="off"
                value={name}
                aria-invalid={error && !name.trim() ? 'true' : undefined}
                onChange={(event) => {
                  setName(event.target.value)
                  if (error) setError(null)
                }}
                className="h-10 rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                placeholder="Waiting place name…"
                disabled={saving}
              />
              <input
                aria-label="Waiting place description"
                name="taskGroupDescription"
                autoComplete="off"
                value={description}
                onChange={(event) => setDescription(event.target.value)}
                className="h-10 rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                placeholder="What should agents use this place for?"
                disabled={saving}
              />
              <div className="flex items-center justify-end gap-2">
                <button
                  type="submit"
                  disabled={saving}
                  className={cn(
                    'inline-flex h-10 items-center justify-center gap-1.5 rounded-full px-4 text-ui-button font-medium text-white transition-transform focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                    'bg-apple-blue hover:bg-apple-blue-focus active:scale-95',
                    saving && 'cursor-not-allowed opacity-60'
                  )}
                >
                  <Check size={14} strokeWidth={2.25} aria-hidden="true" />
                  {saving ? 'Creating…' : 'Create waiting place'}
                </button>
                {agentGroups.length > 0 && (
                  <button
                    type="button"
                    onClick={() => {
                      setFormOpen(false)
                      setError(null)
                    }}
                    disabled={saving}
                    aria-label="Cancel waiting place creation"
                    className="inline-flex h-10 w-10 items-center justify-center rounded-full text-ui-button text-secondary-light transition-transform hover:bg-black/[0.04] hover:text-foreground-light active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
                  >
                    <X size={14} strokeWidth={2.25} aria-hidden="true" />
                  </button>
                )}
              </div>
            </form>
          )}

          {error && (
            <p role="alert" className="text-ui-caption text-apple-red">
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
      className="flex min-h-16 items-center gap-2 rounded-lg bg-white px-2.5 py-2 shadow-sm dark:bg-black/20"
    >
      <span
        className={cn('flex h-8 w-8 shrink-0 items-center justify-center rounded-lg', toneClass)}
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
      className="flex items-center justify-between gap-3 rounded-lg bg-white px-3 py-2 shadow-sm dark:bg-black/20"
    >
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              'shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase',
              TASK_STATE_TONE[task.state]
            )}
          >
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
  if (task.assignedTo) return 'Assigned agent'
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
      return 'Review what the agent finished'
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
    task.params.task,
    task.params.message,
    task.assignedAgentName,
    task.assignedTo,
    task.priority,
    task.state,
    task.error,
    task.blockedHint,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
}
