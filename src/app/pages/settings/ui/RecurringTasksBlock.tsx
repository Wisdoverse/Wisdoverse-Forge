import { useCallback, useEffect, useState } from 'react'
import { CalendarClock, Plus, Trash2 } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useNavigationStore } from '@app/entities/navigation'
import { agentGroupApi, type NavAgentGroup } from '@app/entities/navigation/agent-group'
import {
  orchestrationApi,
  type CreateRecurringTaskInput,
  type RecurringTask,
} from '@app/shared/api/orchestration'

const CADENCE_OPTIONS: { minutes: number; label: string }[] = [
  { minutes: 15, label: 'Every 15 minutes' },
  { minutes: 60, label: 'Every hour' },
  { minutes: 240, label: 'Every 4 hours' },
  { minutes: 1440, label: 'Every day' },
  { minutes: 10080, label: 'Every week' },
]

function cadenceLabel(minutes: number): string {
  return (
    CADENCE_OPTIONS.find((option) => option.minutes === minutes)?.label ?? `Every ${minutes} min`
  )
}

interface Draft {
  name: string
  title: string
  projectId: string
  groupId: string
  cadenceMinutes: number
  requiresApproval: boolean
}

const EMPTY_DRAFT: Draft = {
  name: '',
  title: '',
  projectId: '',
  groupId: '',
  cadenceMinutes: 1440,
  requiresApproval: false,
}

export function RecurringTasksBlock() {
  const navProjects = Object.values(useNavigationStore((s) => s.projects)).flat()
  const [tasks, setTasks] = useState<RecurringTask[]>([])
  const [groups, setGroups] = useState<NavAgentGroup[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [draft, setDraft] = useState<Draft>(EMPTY_DRAFT)
  const [saving, setSaving] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)
  const [confirmingId, setConfirmingId] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setTasks(await orchestrationApi.listRecurringTasks())
    } catch {
      setError('Open Task templates again in a moment. Forge could not load recurring tasks.')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    if (!draft.projectId) {
      setGroups([])
      return
    }
    let cancelled = false
    agentGroupApi
      .getGroups(draft.projectId)
      .then((rows) => {
        if (!cancelled) setGroups(rows)
      })
      .catch(() => {
        if (!cancelled) setGroups([])
      })
    return () => {
      cancelled = true
    }
  }, [draft.projectId])

  function setField<K extends keyof Draft>(key: K, value: Draft[K]) {
    setDraft((current) => ({ ...current, [key]: value }))
  }

  async function handleSave(event: React.FormEvent) {
    event.preventDefault()
    if (saving) return
    setFormError(null)
    const name = draft.name.trim()
    const title = draft.title.trim()
    if (!name) {
      setFormError('Give the recurring task a short name.')
      return
    }
    if (!title) {
      setFormError('Add the task title each run should use.')
      return
    }
    if (!draft.projectId) {
      setFormError('Choose the project where the scheduled task appears.')
      return
    }
    if (!draft.groupId) {
      setFormError('Choose the place in that project where the scheduled task waits.')
      return
    }
    setSaving(true)
    try {
      const input: CreateRecurringTaskInput = {
        name,
        title,
        projectId: draft.projectId,
        groupId: draft.groupId,
        cadenceMinutes: draft.cadenceMinutes,
        requiresApproval: draft.requiresApproval,
      }
      const created = await orchestrationApi.createRecurringTask(input)
      setTasks((current) => [created, ...current])
      setDraft(EMPTY_DRAFT)
    } catch {
      setFormError('Wait a moment, then save again. Forge could not create this schedule.')
    } finally {
      setSaving(false)
    }
  }

  async function toggle(task: RecurringTask) {
    try {
      const updated = await orchestrationApi.updateRecurringTask(task.id, !task.enabled)
      setTasks((current) => current.map((item) => (item.id === task.id ? updated : item)))
    } catch {
      setError('Wait a moment, then try again. Forge could not change this schedule.')
    }
  }

  async function remove(task: RecurringTask) {
    if (confirmingId !== task.id) {
      setConfirmingId(task.id)
      return
    }
    try {
      await orchestrationApi.deleteRecurringTask(task.id)
      setTasks((current) => current.filter((item) => item.id !== task.id))
    } catch {
      setError('Wait a moment, then try again. Forge could not remove this schedule.')
    } finally {
      setConfirmingId(null)
    }
  }

  return (
    <section data-testid="recurring-tasks-block" className="mb-6">
      <h2 className="mb-3 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
        Recurring tasks
      </h2>
      <p className="mb-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
        Schedule a task that Forge re-creates at a fixed cadence; the next available agent starts
        each run.
      </p>

      <form
        onSubmit={handleSave}
        data-testid="recurring-task-create-form"
        className="mb-4 rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-surface-dark"
      >
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label htmlFor="recurring-name" className={uiStyles.label}>
              Name
            </label>
            <input
              id="recurring-name"
              value={draft.name}
              onChange={(event) => setField('name', event.target.value)}
              placeholder="For example: Daily summary"
              className={uiStyles.input}
            />
          </div>
          <div>
            <label htmlFor="recurring-cadence" className={uiStyles.label}>
              Repeat
            </label>
            <select
              id="recurring-cadence"
              value={draft.cadenceMinutes}
              onChange={(event) => setField('cadenceMinutes', Number(event.target.value))}
              className={uiStyles.select}
            >
              {CADENCE_OPTIONS.map((option) => (
                <option key={option.minutes} value={option.minutes}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>
          <div className="sm:col-span-2">
            <label htmlFor="recurring-title" className={uiStyles.label}>
              Task title each run uses
            </label>
            <input
              id="recurring-title"
              value={draft.title}
              onChange={(event) => setField('title', event.target.value)}
              placeholder="For example: Summarize yesterday's work"
              className={uiStyles.input}
            />
          </div>
          <div>
            <label htmlFor="recurring-project" className={uiStyles.label}>
              Project
            </label>
            <select
              id="recurring-project"
              value={draft.projectId}
              onChange={(event) => {
                setField('projectId', event.target.value)
                setField('groupId', '')
              }}
              className={uiStyles.select}
            >
              <option value="">Choose a project</option>
              {navProjects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label htmlFor="recurring-group" className={uiStyles.label}>
              Place for new tasks
            </label>
            <select
              id="recurring-group"
              value={draft.groupId}
              onChange={(event) => setField('groupId', event.target.value)}
              className={uiStyles.select}
            >
              <option value="">Choose a place</option>
              {groups.map((group) => (
                <option key={group.id} value={group.id}>
                  {group.name}
                </option>
              ))}
            </select>
          </div>
        </div>
        <label className="mt-3 flex items-start gap-2 text-ui-body text-secondary-light dark:text-secondary-dark">
          <input
            type="checkbox"
            checked={draft.requiresApproval}
            onChange={(event) => setField('requiresApproval', event.target.checked)}
            className="mt-0.5"
          />
          <span>Wait for my approval before the agent starts each run</span>
        </label>
        {formError && (
          <p role="alert" aria-live="assertive" className="mt-3 text-ui-caption text-apple-red">
            {formError}
          </p>
        )}
        <button
          type="submit"
          disabled={saving}
          data-testid="save-recurring-task"
          className={cn(uiStyles.primaryButton, 'mt-4')}
        >
          <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
          {saving ? 'Saving...' : 'Schedule task'}
        </button>
      </form>

      {error && (
        <div role="alert" aria-live="assertive" className={cn(uiStyles.error, 'text-ui-caption')}>
          {error}
        </div>
      )}

      {!loading && (tasks?.length ?? 0) === 0 ? (
        <div className={uiStyles.cardPadded}>
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            No recurring tasks yet. Create one above; it will appear as a new task on each schedule.
          </p>
        </div>
      ) : (
        <ul
          data-testid="recurring-task-list"
          className={cn(uiStyles.card, 'divide-y divide-black/[0.06] dark:divide-white/[0.08]')}
        >
          {tasks.map((task) => (
            <li
              key={task.id}
              data-testid={`recurring-task-${task.id}`}
              className="flex items-center justify-between gap-3 px-4 py-3"
            >
              <div className="min-w-0">
                <span className="flex items-center gap-2">
                  <CalendarClock
                    size={14}
                    strokeWidth={2}
                    className="shrink-0 text-apple-blue"
                    aria-hidden="true"
                  />
                  <span className="truncate font-medium text-foreground-light dark:text-foreground-dark">
                    {task.name}
                  </span>
                  <span className={uiStyles.badge}>{cadenceLabel(task.cadenceMinutes)}</span>
                </span>
                <span className="mt-0.5 block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {task.title}
                  {task.enabled ? '' : ' · paused'}
                </span>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  onClick={() => void toggle(task)}
                  data-testid={`toggle-recurring-task-${task.id}`}
                  className={cn(uiStyles.subtleButton)}
                >
                  {task.enabled ? 'Pause' : 'Resume'}
                </button>
                <button
                  type="button"
                  onClick={() => void remove(task)}
                  data-testid={`delete-recurring-task-${task.id}`}
                  className={cn(
                    'inline-flex h-8 shrink-0 items-center gap-1.5 rounded-button px-2.5 text-ui-button font-medium transition-colors',
                    confirmingId === task.id
                      ? 'border border-apple-red/30 bg-apple-red/10 text-apple-red'
                      : 'text-secondary-light hover:text-apple-red dark:text-secondary-dark'
                  )}
                >
                  <Trash2 size={14} strokeWidth={2} aria-hidden="true" />
                  {confirmingId === task.id ? 'Confirm remove' : 'Remove'}
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
