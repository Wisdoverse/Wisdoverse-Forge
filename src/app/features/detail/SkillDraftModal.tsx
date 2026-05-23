import { useEffect, useMemo, useState } from 'react'
import { X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSkillsStore } from '@app/shared/model/skills.store'
import type { TaskResultArtifact, TaskSummary } from '@app/shared/api/orchestration'

interface SkillDraftModalProps {
  open: boolean
  task: TaskSummary
  artifacts: TaskResultArtifact[]
  onClose: () => void
}

interface DraftForm {
  name: string
  description: string
  triggerPattern: string
  content: string
}

export function SkillDraftModal({ open, task, artifacts, onClose }: SkillDraftModalProps) {
  const createSkill = useSkillsStore((state) => state.createSkill)
  const initialForm = useMemo(() => buildSkillDraft(task, artifacts), [artifacts, task])
  const [form, setForm] = useState<DraftForm>(initialForm)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    setForm(initialForm)
    setError(null)
  }, [initialForm, open])

  if (!open) return null

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const name = form.name.trim()
    const content = form.content.trim()
    if (!name) {
      setError('Name is required')
      return
    }
    if (!content) {
      setError('Content is required')
      return
    }

    setSubmitting(true)
    setError(null)
    try {
      await createSkill({
        name,
        description: form.description.trim() || undefined,
        trigger_pattern: form.triggerPattern.trim() || undefined,
        content,
      })
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create skill')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center p-4 sm:items-center">
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="skill-draft-title"
        className={cn(
          'relative flex max-h-[88vh] w-full max-w-2xl flex-col overflow-y-auto',
          'rounded-card border border-black/[0.08] bg-white p-5 dark:border-white/[0.1] dark:bg-[#2c2c2e]'
        )}
      >
        <div className="mb-4 flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 id="skill-draft-title" className={uiStyles.sectionTitle}>
              Draft reusable skill
            </h2>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Review the extracted instructions before publishing them to the workspace skill
              library.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close dialog"
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-secondary-light transition-colors hover:bg-black/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/40 dark:text-secondary-dark dark:hover:bg-white/5"
          >
            <X size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        </div>

        {error && <div className={uiStyles.error}>{error}</div>}

        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label htmlFor="skill-draft-name" className={uiStyles.label}>
                Name
              </label>
              <input
                id="skill-draft-name"
                value={form.name}
                onChange={(event) =>
                  setForm((current) => ({ ...current, name: event.target.value }))
                }
                className={uiStyles.input}
              />
            </div>
            <div>
              <label htmlFor="skill-draft-trigger" className={uiStyles.label}>
                Trigger Pattern
              </label>
              <input
                id="skill-draft-trigger"
                value={form.triggerPattern}
                onChange={(event) =>
                  setForm((current) => ({ ...current, triggerPattern: event.target.value }))
                }
                className={cn(uiStyles.input, 'font-mono')}
              />
            </div>
          </div>

          <div>
            <label htmlFor="skill-draft-description" className={uiStyles.label}>
              Description
            </label>
            <input
              id="skill-draft-description"
              value={form.description}
              onChange={(event) =>
                setForm((current) => ({ ...current, description: event.target.value }))
              }
              className={uiStyles.input}
            />
          </div>

          <div>
            <label htmlFor="skill-draft-content" className={uiStyles.label}>
              Content
            </label>
            <textarea
              id="skill-draft-content"
              value={form.content}
              onChange={(event) =>
                setForm((current) => ({ ...current, content: event.target.value }))
              }
              className="min-h-64 w-full resize-y rounded-[18px] border border-black/[0.08] bg-white px-3 py-2 font-mono text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70"
            />
          </div>

          <div className="flex justify-end gap-2 pt-1">
            <button type="button" onClick={onClose} className={uiStyles.secondaryButton}>
              Cancel
            </button>
            <button type="submit" disabled={submitting} className={uiStyles.primaryButton}>
              {submitting ? 'Publishing...' : 'Publish skill'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

function buildSkillDraft(task: TaskSummary, artifacts: TaskResultArtifact[]): DraftForm {
  const title = task.params.task.trim() || `Task ${task.id.slice(0, 8)}`
  const artifactContent = artifacts
    .map((artifact) => `# ${artifact.name}\n\n${artifact.data}`)
    .join('\n\n---\n\n')
    .trim()
  const source = artifactContent || task.params.message.trim() || title

  return {
    name: slugify(title) || `task-${task.id.slice(0, 8)}-skill`,
    description: `Reusable instructions extracted from completed task: ${title}`,
    triggerPattern: title.toLowerCase().slice(0, 80),
    content: `# Skill: ${title}

## When to use
Use this skill when a future task needs the same judgment, workflow, or implementation pattern.

## Reusable instructions
${source}`,
  }
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64)
}
