import { useCallback, useEffect, useState } from 'react'
import { ClipboardCheck, Plus, Trash2 } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import { useNavigationStore } from '@app/entities/navigation'
import { RecurringTasksBlock } from './RecurringTasksBlock'
import {
  orchestrationApi,
  type CreateTaskTemplateInput,
  type TaskTemplate,
} from '@app/shared/api/orchestration'

const PRIORITY_OPTIONS: { value: string; label: string }[] = [
  { value: 'low', label: 'Low' },
  { value: 'normal', label: 'Normal' },
  { value: 'high', label: 'High' },
  { value: 'urgent', label: 'Urgent' },
]

function priorityLabel(value: string): string {
  return PRIORITY_OPTIONS.find((option) => option.value === value)?.label ?? value
}

interface TemplateDraft {
  name: string
  title: string
  description: string
  priority: string
  requiresApproval: boolean
  projectId: string
}

const EMPTY_DRAFT: TemplateDraft = {
  name: '',
  title: '',
  description: '',
  priority: 'normal',
  requiresApproval: false,
  projectId: '',
}

export function TaskTemplatesSection() {
  const navProjects = Object.values(useNavigationStore((s) => s.projects)).flat()
  const [templates, setTemplates] = useState<TaskTemplate[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [draft, setDraft] = useState<TemplateDraft>(EMPTY_DRAFT)
  const [saving, setSaving] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)
  const [confirmingId, setConfirmingId] = useState<string | null>(null)
  const [deletingId, setDeletingId] = useState<string | null>(null)

  const loadTemplates = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setTemplates(await orchestrationApi.listTaskTemplates())
    } catch {
      setError('Open Task templates again in a moment. Forge could not load the saved templates.')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadTemplates()
  }, [loadTemplates])

  async function handleSave(event: React.FormEvent) {
    event.preventDefault()
    if (saving) return
    setFormError(null)
    const name = draft.name.trim()
    const title = draft.title.trim()
    if (!name) {
      setFormError('Give the template a short name so people can find it.')
      return
    }
    if (!title) {
      setFormError('Add the task title this template writes.')
      return
    }
    setSaving(true)
    try {
      const input: CreateTaskTemplateInput = {
        name,
        title,
        description: draft.description,
        priority: draft.priority,
        requiresApproval: draft.requiresApproval,
        ...(draft.projectId ? { projectId: draft.projectId } : {}),
      }
      const created = await orchestrationApi.createTaskTemplate(input)
      setTemplates((current) => [created, ...current])
      setDraft(EMPTY_DRAFT)
    } catch {
      setFormError('Wait a moment, then save again. Forge could not save this template.')
    } finally {
      setSaving(false)
    }
  }

  async function handleDelete(template: TaskTemplate) {
    if (deletingId) return
    if (confirmingId !== template.id) {
      setConfirmingId(template.id)
      return
    }
    setDeletingId(template.id)
    try {
      await orchestrationApi.deleteTaskTemplate(template.id)
      setTemplates((current) => current.filter((item) => item.id !== template.id))
    } catch {
      setError('Wait a moment, then try removing it again. Forge could not delete this template.')
    } finally {
      setDeletingId(null)
      setConfirmingId(null)
    }
  }

  function setField<K extends keyof TemplateDraft>(key: K, value: TemplateDraft[K]) {
    setDraft((current) => ({ ...current, [key]: value }))
  }

  return (
    <div>
      <header className="mb-4">
        <h1 className={uiStyles.sectionTitle}>Task templates</h1>
        <p className={uiStyles.sectionDescription}>
          Save a reusable task brief your team can apply when writing a task. Templates appear in
          the task form under <span className="font-medium">Saved by your team</span>.
        </p>
      </header>

      <form
        onSubmit={handleSave}
        data-testid="task-template-create-form"
        className="mb-6 rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-surface-dark"
      >
        <h2 className="mb-3 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          New template
        </h2>
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label htmlFor="template-name" className={uiStyles.label}>
              Template name
            </label>
            <input
              id="template-name"
              value={draft.name}
              onChange={(event) => setField('name', event.target.value)}
              placeholder="For example: Ship a small feature"
              className={uiStyles.input}
            />
          </div>
          <div>
            <label htmlFor="template-priority" className={uiStyles.label}>
              Priority
            </label>
            <select
              id="template-priority"
              value={draft.priority}
              onChange={(event) => setField('priority', event.target.value)}
              className={uiStyles.select}
            >
              {PRIORITY_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label htmlFor="template-project" className={uiStyles.label}>
              Use in project
            </label>
            <select
              id="template-project"
              value={draft.projectId}
              onChange={(event) => setField('projectId', event.target.value)}
              className={uiStyles.select}
            >
              <option value="">All projects (team-wide)</option>
              {navProjects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </div>
          <div className="sm:col-span-2">
            <label htmlFor="template-title" className={uiStyles.label}>
              Task title it writes
            </label>
            <input
              id="template-title"
              value={draft.title}
              onChange={(event) => setField('title', event.target.value)}
              placeholder="For example: Fix the login error"
              className={uiStyles.input}
            />
          </div>
          <div className="sm:col-span-2">
            <label htmlFor="template-description" className={uiStyles.label}>
              Task brief it fills in
            </label>
            <textarea
              id="template-description"
              value={draft.description}
              onChange={(event) => setField('description', event.target.value)}
              rows={5}
              placeholder="What should change, where to work, and what proof looks finished."
              className={cn(uiStyles.input, 'h-auto py-2')}
            />
          </div>
        </div>
        <label className="mt-3 flex items-start gap-2 text-ui-body text-secondary-light dark:text-secondary-dark">
          <input
            type="checkbox"
            checked={draft.requiresApproval}
            onChange={(event) => setField('requiresApproval', event.target.checked)}
            className="mt-0.5"
          />
          <span>
            Wait for my approval before the agent starts
            <span className="block text-ui-caption">
              The task waits until the person who created it approves the plan.
            </span>
          </span>
        </label>
        {formError && (
          <p role="alert" aria-live="assertive" className="mt-3 text-ui-caption text-apple-red">
            {formError}
          </p>
        )}
        <button
          type="submit"
          disabled={saving}
          data-testid="save-task-template"
          className={cn(uiStyles.primaryButton, 'mt-4')}
        >
          <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
          {saving ? 'Saving...' : 'Save template'}
        </button>
      </form>

      {error && (
        <div role="alert" aria-live="assertive" className={cn(uiStyles.error, 'text-ui-caption')}>
          {error}
          <button
            type="button"
            onClick={() => void loadTemplates()}
            className={cn(uiStyles.subtleButton, 'ml-2')}
          >
            Try again
          </button>
        </div>
      )}

      {loading ? (
        <BeginnerLoadingState
          title="Loading saved templates..."
          detail="A moment while Forge checks your team space for saved templates."
          nextStep="If this takes long, reopen this page and try again."
          compact
        />
      ) : templates.length === 0 ? (
        <div className={uiStyles.cardPadded}>
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            No saved templates yet. Create the first one above; it will appear in the task form
            under <span className="font-medium">Saved by your team</span>.
          </p>
        </div>
      ) : (
        <ul
          data-testid="task-template-list"
          className={cn(uiStyles.card, 'divide-y divide-black/[0.06] dark:divide-white/[0.08]')}
        >
          {templates.map((template) => (
            <li
              key={template.id}
              data-testid={`task-template-${template.id}`}
              className="flex items-center justify-between gap-3 px-4 py-3"
            >
              <div className="min-w-0">
                <span className="flex items-center gap-2">
                  <ClipboardCheck
                    size={14}
                    strokeWidth={2}
                    className="shrink-0 text-apple-blue"
                    aria-hidden="true"
                  />
                  <span className="truncate font-medium text-foreground-light dark:text-foreground-dark">
                    {template.name}
                  </span>
                  <span className={uiStyles.badge}>{priorityLabel(template.priority)}</span>
                </span>
                <span className="mt-0.5 block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {template.title}
                  {template.requiresApproval ? ' · waits for approval' : ''}
                </span>
              </div>
              <button
                type="button"
                onClick={() => void handleDelete(template)}
                disabled={deletingId === template.id}
                data-testid={`delete-task-template-${template.id}`}
                className={cn(
                  'inline-flex h-8 shrink-0 items-center gap-1.5 rounded-button px-2.5 text-ui-button font-medium transition-colors',
                  confirmingId === template.id
                    ? 'border border-apple-red/30 bg-apple-red/10 text-apple-red'
                    : 'text-secondary-light hover:text-apple-red dark:text-secondary-dark'
                )}
              >
                <Trash2 size={14} strokeWidth={2} aria-hidden="true" />
                {confirmingId === template.id ? 'Confirm remove' : 'Remove'}
              </button>
            </li>
          ))}
        </ul>
      )}

      <RecurringTasksBlock />
    </div>
  )
}
