import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowRight, CheckCircle2, LibraryBig, Users, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSkillsStore, type Skill } from '@app/shared/model/skills.store'
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

const SKILL_REVIEW_POINTS = [
  { label: 'Reusable rule', value: 'Keep only instructions future work should repeat.' },
  {
    label: 'No secrets',
    value: 'Remove tokens, customer data, one-time paths, and private notes.',
  },
  { label: 'Next owner', value: 'After publishing, choose the agents that should use it.' },
]

export function SkillDraftModal({ open, task, artifacts, onClose }: SkillDraftModalProps) {
  const createSkill = useSkillsStore((state) => state.createSkill)
  const initialForm = useMemo(() => buildSkillDraft(task, artifacts), [artifacts, task])
  const [form, setForm] = useState<DraftForm>(initialForm)
  const [createdSkill, setCreatedSkill] = useState<Skill | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [fieldError, setFieldError] = useState<'name' | 'content' | null>(null)
  const nameInputRef = useRef<HTMLInputElement>(null)
  const contentInputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    if (!open) return
    setForm(initialForm)
    setCreatedSkill(null)
    setError(null)
    setFieldError(null)
  }, [initialForm, open])

  if (!open) return null

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const name = form.name.trim()
    const content = form.content.trim()
    if (!name) {
      setError('Skill name is required')
      return
    }
    if (!content) {
      setError('Reusable instructions are required')
      return
    }

    setSubmitting(true)
    setError(null)
    setFieldError(null)
    try {
      const skill = await createSkill({
        name,
        description: form.description.trim() || undefined,
        trigger_pattern: form.triggerPattern.trim() || undefined,
        content,
      })
      setCreatedSkill(skill)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create skill')
    } finally {
      setSubmitting(false)
    }
  }

  function updateField(field: keyof DraftForm, value: string) {
    setForm((current) => ({ ...current, [field]: value }))
    if (field === fieldError) {
      setError(null)
      setFieldError(null)
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
              Turn this completed task into reusable instructions. Review what should repeat before
              publishing it to the workspace skill library.
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

        {error && (
          <div id="skill-draft-error" role="alert" className={uiStyles.error}>
            {error}
          </div>
        )}

        {createdSkill ? (
          <SkillPublishedState skill={createdSkill} onClose={onClose} />
        ) : (
          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            <div className="rounded-card border border-apple-blue/20 bg-apple-blue/10 px-3 py-2 text-ui-caption text-apple-blue">
              Check 3 things before publishing: the name is recognizable, the trigger words match
              future work, and the instructions can stand alone without this task open.
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              <div>
                <label htmlFor="skill-draft-name" className={uiStyles.label}>
                  Skill name
                </label>
                <input
                  id="skill-draft-name"
                  ref={nameInputRef}
                  name="skillDraftName"
                  autoComplete="off"
                  value={form.name}
                  onChange={(event) => updateField('name', event.target.value)}
                  aria-invalid={fieldError === 'name'}
                  aria-describedby={
                    fieldError === 'name'
                      ? 'skill-draft-name-help skill-draft-error'
                      : 'skill-draft-name-help'
                  }
                  className={uiStyles.input}
                />
                <p
                  id="skill-draft-name-help"
                  className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
                >
                  Use a short name people will understand in the skill list.
                </p>
              </div>
              <div>
                <label htmlFor="skill-draft-trigger" className={uiStyles.label}>
                  Use when
                </label>
                <p
                  id="skill-draft-trigger-help"
                  className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
                >
                  Short phrase that tells agents when this skill fits.
                </p>
                <input
                  id="skill-draft-trigger"
                  name="skillDraftTriggerPattern"
                  autoComplete="off"
                  value={form.triggerPattern}
                  onChange={(event) => updateField('triggerPattern', event.target.value)}
                  aria-describedby="skill-draft-trigger-help"
                  className={cn(uiStyles.input, 'font-mono')}
                />
                <p
                  id="skill-draft-trigger-help"
                  className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
                >
                  Optional. Keep the words future users are likely to search.
                </p>
              </div>
            </div>

            <div>
              <label htmlFor="skill-draft-description" className={uiStyles.label}>
                Short description
              </label>
              <input
                id="skill-draft-description"
                name="skillDraftDescription"
                autoComplete="off"
                value={form.description}
                onChange={(event) => updateField('description', event.target.value)}
                aria-describedby="skill-draft-description-help"
                className={uiStyles.input}
              />
              <p
                id="skill-draft-description-help"
                className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
              >
                Optional. Say what this reusable instruction helps people do.
              </p>
            </div>

            <div className="rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2.5 dark:border-white/[0.08] dark:bg-white/[0.04]">
              <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Check before publishing
              </div>
              <div className="mt-2 grid gap-1.5 sm:grid-cols-3">
                {SKILL_REVIEW_POINTS.map((point) => (
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

            <div>
              <label htmlFor="skill-draft-content" className={uiStyles.label}>
                Reusable instructions
              </label>
              <textarea
                id="skill-draft-content"
                ref={contentInputRef}
                name="skillDraftContent"
                autoComplete="off"
                value={form.content}
                onChange={(event) => updateField('content', event.target.value)}
                aria-invalid={fieldError === 'content'}
                aria-describedby={
                  fieldError === 'content'
                    ? 'skill-draft-content-help skill-draft-error'
                    : 'skill-draft-content-help'
                }
                className="min-h-64 w-full resize-y rounded-[18px] border border-black/[0.08] bg-white px-3 py-2 font-mono text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70"
              />
              <p
                id="skill-draft-content-help"
                className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
              >
                Required. Remove task-specific details and keep only reusable instructions.
              </p>
            </div>

            <div className="flex justify-end gap-2 pt-1">
              <button type="button" onClick={onClose} className={uiStyles.secondaryButton}>
                Cancel
              </button>
              <button type="submit" disabled={submitting} className={uiStyles.primaryButton}>
                {submitting ? 'Publishing…' : 'Publish Skill'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  )
}

function SkillPublishedState({ skill, onClose }: { skill: Skill; onClose: () => void }) {
  return (
    <div className="flex flex-col gap-4" data-testid="skill-published-state">
      <div className="rounded-card border border-apple-green/20 bg-apple-green/10 px-4 py-3 text-apple-green">
        <div className="flex items-start gap-3">
          <CheckCircle2 size={18} strokeWidth={2.25} aria-hidden="true" className="mt-0.5" />
          <div className="min-w-0">
            <p className="text-ui-section font-semibold">Skill published</p>
            <p className="mt-1 break-words text-ui-body text-foreground-light dark:text-foreground-dark">
              {skill.name}
            </p>
          </div>
        </div>
      </div>

      <div className="grid gap-2 sm:grid-cols-2">
        <NextReuseLink
          href="/skills"
          Icon={LibraryBig}
          title="Open skills"
          detail="Review the reusable instructions."
        />
        <NextReuseLink
          href="/agents"
          Icon={Users}
          title="Choose agent"
          detail="Pick who should reuse this skill."
        />
      </div>

      <div className="flex justify-end">
        <button type="button" onClick={onClose} className={uiStyles.secondaryButton}>
          Done
        </button>
      </div>
    </div>
  )
}

function NextReuseLink({
  href,
  Icon,
  title,
  detail,
}: {
  href: string
  Icon: typeof LibraryBig
  title: string
  detail: string
}) {
  return (
    <a
      href={href}
      className="group flex min-w-0 items-start gap-3 rounded-card border border-black/[0.08] bg-black/[0.02] px-3 py-3 transition-colors hover:border-apple-blue/35 hover:bg-apple-blue/10 dark:border-white/[0.1] dark:bg-white/[0.035] dark:hover:bg-apple-blue/15"
    >
      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-white text-apple-blue dark:bg-white/[0.08]">
        <Icon size={16} strokeWidth={2.25} aria-hidden="true" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </span>
        <span className="mt-0.5 block text-ui-caption text-secondary-light dark:text-secondary-dark">
          {detail}
        </span>
      </span>
      <ArrowRight
        size={14}
        strokeWidth={2.25}
        aria-hidden="true"
        className="mt-1 shrink-0 text-secondary-light transition-colors group-hover:text-apple-blue dark:text-secondary-dark"
      />
    </a>
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
