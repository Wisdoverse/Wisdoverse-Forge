import { type FormEvent, useEffect, useMemo, useState } from 'react'
import {
  Check,
  ClipboardCheck,
  Layers3,
  Plus,
  ShieldCheck,
  Wrench,
  X,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { useBoardStore } from '@app/shared/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'

const DEFAULT_GROUP_DESCRIPTION = 'Agents in this group can receive tasks from the board.'

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
    name: 'Delivery Group',
    description: 'Build scoped changes, keep work moving, and verify before handoff.',
    Icon: Wrench,
  },
  {
    id: 'review',
    label: 'Review',
    summary: 'Risk and readiness',
    name: 'Review Group',
    description: 'Review completed work for regressions, missing tests, and release risk.',
    Icon: ShieldCheck,
  },
  {
    id: 'triage',
    label: 'Triage',
    summary: 'Clarify and route',
    name: 'Triage Group',
    description: 'Clarify incoming work, identify blockers, and route tasks to the right agent.',
    Icon: ClipboardCheck,
  },
]

export function AgentGroupsPanel() {
  const selectedProjectId = useNavigationStore((state) => state.selectedProjectId)
  const projectsByTeam = useNavigationStore((state) => state.projects)
  const agentGroups = useNavigationStore((state) => state.agentGroups)
  const createAgentGroup = useNavigationStore((state) => state.createAgentGroup)
  const selectedGroupId = useBoardStore((state) => state.selectedGroupId)
  const setSelectedGroupId = useBoardStore((state) => state.setSelectedGroupId)
  const [formOpen, setFormOpen] = useState(false)
  const [name, setName] = useState('')
  const [description, setDescription] = useState(DEFAULT_GROUP_DESCRIPTION)
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const selectedProject = useMemo(() => {
    if (!selectedProjectId) return null
    return (
      Object.values(projectsByTeam)
        .flat()
        .find((project) => project.id === selectedProjectId) ?? null
    )
  }, [projectsByTeam, selectedProjectId])

  useEffect(() => {
    setName('')
    setDescription(DEFAULT_GROUP_DESCRIPTION)
    setSelectedTemplateId(null)
    setError(null)
  }, [selectedProjectId])

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
      setError('Select a project before creating a task group.')
      return
    }

    const trimmedName = name.trim()
    if (!trimmedName) {
      setError('Task group name is required.')
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
      setError(err instanceof Error ? err.message : 'Failed to create task group.')
    } finally {
      setSaving(false)
    }
  }

  function applyTemplate(template: TaskGroupTemplate) {
    setSelectedTemplateId(template.id)
    setName(template.name)
    setDescription(template.description)
  }

  return (
    <section
      data-testid="agent-groups-panel"
      className="rounded-card border border-black/[0.08] bg-white p-6 dark:border-white/[0.1] dark:bg-[#2a2a2c] xl:sticky xl:top-0 xl:self-start"
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
              Task Routing
            </h2>
          </div>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Route board tasks to agent groups in the selected project.
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
            New
          </button>
        )}
      </div>

      {!selectedProjectId ? (
        <div className="mt-3 rounded-lg border border-dashed border-black/10 px-3 py-3 text-ui-caption text-secondary-light dark:border-white/10 dark:text-secondary-dark">
          Select a project from the sidebar to manage task routing.
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
                No task groups yet
              </div>
            )}
          </div>

          {formOpen && (
            <form onSubmit={handleCreateGroup} className="grid gap-2">
              <div
                role="group"
                aria-label="Task group templates"
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
                aria-label="Task group name"
                name="taskGroupName"
                autoComplete="off"
                value={name}
                onChange={(event) => setName(event.target.value)}
                className="h-10 rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                placeholder="Task group name…"
                disabled={saving}
              />
              <input
                aria-label="Task group description"
                name="taskGroupDescription"
                autoComplete="off"
                value={description}
                onChange={(event) => setDescription(event.target.value)}
                className="h-10 rounded-full border border-black/[0.08] bg-white px-4 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark"
                placeholder="Describe task routing…"
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
                  {saving ? 'Creating…' : 'Create'}
                </button>
                {agentGroups.length > 0 && (
                  <button
                    type="button"
                    onClick={() => {
                      setFormOpen(false)
                      setError(null)
                    }}
                    disabled={saving}
                    aria-label="Cancel task group creation"
                    className="inline-flex h-10 w-10 items-center justify-center rounded-full text-ui-button text-secondary-light transition-transform hover:bg-black/[0.04] hover:text-foreground-light active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
                  >
                    <X size={14} strokeWidth={2.25} aria-hidden="true" />
                  </button>
                )}
              </div>
            </form>
          )}

          {error && <p className="text-ui-caption text-apple-red">{error}</p>}
        </div>
      )}
    </section>
  )
}
