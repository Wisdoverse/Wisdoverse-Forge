import { useEffect, useRef, useState, type FormEvent } from 'react'
import { Sparkles, X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSkillsStore } from '@app/shared/model/skills.store'
import { createSkillErrorMessage } from './model/createSkillErrorMessage'

interface CreateSkillModalProps {
  open: boolean
  onClose: () => void
}

const emptyForm = {
  name: '',
  description: '',
  triggerPattern: '',
  content: '',
}

const SKILL_REVIEW_POINTS = [
  { label: 'Repeatable', value: 'Use this for work your team expects to repeat.' },
  {
    label: 'Safe to share',
    value: 'Leave out tokens, private notes, and one-time project details.',
  },
  { label: 'Agent ready', value: 'Write steps an agent can follow without extra context.' },
]

const skillTemplates = [
  {
    id: 'review',
    label: 'Review checklist',
    description: 'Review before handoff',
    form: {
      name: 'review-checklist',
      description: 'Check work before handoff',
      triggerPattern: 'review',
      content:
        'Check what changed.\nList risks, missing tests, and the next safe action.\nKeep the answer short and link evidence when available.',
    },
  },
  {
    id: 'release',
    label: 'Release notes',
    description: 'Draft a clear summary',
    form: {
      name: 'release-notes',
      description: 'Draft release notes from accepted work',
      triggerPattern: 'release',
      content:
        'Summarize what changed.\nGroup user-facing updates, fixes, and risks.\nCall out any setup or migration step before publishing.',
    },
  },
  {
    id: 'ci-status',
    label: 'CI status check',
    description: 'Check builds without waiting',
    form: {
      name: 'ci-status-check',
      description: 'Summarize build status from one fresh check',
      triggerPattern: 'ci status, checks, build status',
      content:
        'Check GitHub or GitLab build status once.\nSummarize whether it passed, failed, or is still running.\nIf it is still running, give the next safe time to check instead of watching it repeatedly.',
    },
  },
  {
    id: 'incident',
    label: 'Incident notes',
    description: 'Capture facts and next steps',
    form: {
      name: 'incident-notes',
      description: 'Record incident facts and recovery steps',
      triggerPattern: 'incident',
      content:
        'Record the current impact, suspected cause, and timeline.\nList the safest next action.\nSeparate confirmed facts from assumptions.',
    },
  },
]

export function CreateSkillModal({ open, onClose }: CreateSkillModalProps) {
  const createSkill = useSkillsStore((state) => state.createSkill)
  const [form, setForm] = useState(emptyForm)
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [fieldError, setFieldError] = useState<'name' | 'content' | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const nameInputRef = useRef<HTMLInputElement>(null)
  const contentInputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    if (!open) return
    setForm(emptyForm)
    setSelectedTemplateId(null)
    setError(null)
    setFieldError(null)

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose()
    }

    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [onClose, open])

  if (!open) return null

  function updateField(field: keyof typeof emptyForm, value: string) {
    setForm((current) => ({ ...current, [field]: value }))
    if (field === fieldError || field === 'name' || field === 'content') {
      setError(null)
      setFieldError(null)
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const name = form.name.trim()
    const content = form.content.trim()

    if (!name) {
      setError('Name this skill before creating it.')
      nameInputRef.current?.focus()
      return
    }
    if (!content) {
      setError('Add the instructions this skill should apply.')
      contentInputRef.current?.focus()
      return
    }

    setSubmitting(true)
    setError(null)
    setFieldError(null)
    try {
      await createSkill({
        name,
        description: form.description.trim() || undefined,
        trigger_pattern: form.triggerPattern.trim() || undefined,
        content,
      })
      onClose()
    } catch (err) {
      setError(createSkillErrorMessage(err))
    } finally {
      setSubmitting(false)
    }
  }

  function applyTemplate(template: (typeof skillTemplates)[number]) {
    setForm(template.form)
    setSelectedTemplateId(template.id)
    setError(null)
  }

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center p-4 sm:items-center">
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-skill-title"
        className={cn(
          'relative flex max-h-[86vh] w-full max-w-lg flex-col overflow-y-auto',
          'rounded-card border border-black/[0.08] bg-white p-5 dark:border-white/[0.1] dark:bg-[#2c2c2e]'
        )}
      >
        <div className="mb-4 flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 id="create-skill-title" className={uiStyles.sectionTitle}>
              New saved instruction
            </h2>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Save instructions your agents can reuse on future tasks. Keep it general and safe
              enough for the workspace.
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

        <p className="mb-4 text-ui-body text-secondary-light dark:text-secondary-dark">
          Skills are reusable instructions. Start with a clear name and the rules the agent should
          follow.
        </p>

        {error && (
          <div id="create-skill-error" role="alert" className={uiStyles.error}>
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <section aria-labelledby="skill-template-title" className="space-y-2">
            <div className="flex items-center gap-2">
              <Sparkles
                size={14}
                strokeWidth={2.25}
                className="text-apple-blue"
                aria-hidden="true"
              />
              <h3
                id="skill-template-title"
                className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark"
              >
                Common starting points
              </h3>
            </div>
            <div role="group" aria-label="Skill templates" className="grid gap-2 sm:grid-cols-2">
              {skillTemplates.map((template) => (
                <button
                  key={template.id}
                  type="button"
                  aria-pressed={selectedTemplateId === template.id}
                  onClick={() => applyTemplate(template)}
                  className={cn(
                    'rounded-lg border px-3 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35',
                    selectedTemplateId === template.id
                      ? 'border-apple-blue/45 bg-apple-blue/[0.08]'
                      : 'border-black/[0.08] bg-white hover:border-apple-blue/35 dark:border-white/[0.1] dark:bg-white/[0.04]'
                  )}
                >
                  <span className="block truncate text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
                    {template.label}
                  </span>
                  <span className="mt-0.5 block text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {template.description}
                  </span>
                </button>
              ))}
            </div>
          </section>

          <div>
            <label htmlFor="skill-name" className={uiStyles.label}>
              Instruction name
            </label>
            <input
              id="skill-name"
              ref={nameInputRef}
              value={form.name}
              onChange={(event) => updateField('name', event.target.value)}
              className={uiStyles.input}
              placeholder="e.g. release-review"
              autoFocus
            />
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Use a short name people can recognize later.
            </p>
          </div>

          <div>
            <label htmlFor="skill-description" className={uiStyles.label}>
              Short description
            </label>
            <input
              id="skill-description"
              value={form.description}
              onChange={(event) => updateField('description', event.target.value)}
              className={uiStyles.input}
              placeholder="Short summary shown in the saved instruction list"
            />
            <p
              id="skill-description-help"
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              Optional. One sentence is enough.
            </p>
          </div>

          <div>
            <label htmlFor="skill-trigger" className={uiStyles.label}>
              Matching words for future tasks
            </label>
            <p
              id="skill-trigger-help"
              className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              Type the words people usually write when this skill should be suggested.
            </p>
            <input
              id="skill-trigger"
              value={form.triggerPattern}
              onChange={(event) => updateField('triggerPattern', event.target.value)}
              className={uiStyles.input}
              placeholder="release review, incident notes"
            />
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Leave blank if people should choose the skill manually.
            </p>
          </div>

          <div className="rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2.5 dark:border-white/[0.08] dark:bg-white/[0.04]">
            <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Check before creating
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
            <label htmlFor="skill-content" className={uiStyles.label}>
              Agent instructions
            </label>
            <textarea
              id="skill-content"
              ref={contentInputRef}
              value={form.content}
              onChange={(event) => updateField('content', event.target.value)}
              className={cn(
                'min-h-36 w-full resize-y rounded-[18px] border border-black/[0.08] bg-white px-3 py-2 font-mono text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70'
              )}
              placeholder="Steps the agent should follow when this skill is selected"
            />
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Plain steps work best. Include what success should look like.
            </p>
          </div>

          <div className="flex justify-end gap-2 pt-1">
            <button type="button" onClick={onClose} className={uiStyles.secondaryButton}>
              Cancel
            </button>
            <button type="submit" disabled={submitting} className={uiStyles.primaryButton}>
              {submitting ? 'Creating...' : 'Create Skill'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
