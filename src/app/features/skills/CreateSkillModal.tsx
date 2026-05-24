import { useEffect, useRef, useState } from 'react'
import { X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSkillsStore } from '@app/shared/model/skills.store'

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

export function CreateSkillModal({ open, onClose }: CreateSkillModalProps) {
  const createSkill = useSkillsStore((state) => state.createSkill)
  const [form, setForm] = useState(emptyForm)
  const [error, setError] = useState<string | null>(null)
  const [fieldError, setFieldError] = useState<'name' | 'content' | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const nameInputRef = useRef<HTMLInputElement>(null)
  const contentInputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    if (!open) return
    setForm(emptyForm)
    setError(null)
    setFieldError(null)

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose()
    }

    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [onClose, open])

  if (!open) return null

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const name = form.name.trim()
    const content = form.content.trim()

    if (!name) {
      setError('Name this skill before creating it.')
      setFieldError('name')
      nameInputRef.current?.focus()
      return
    }
    if (!content) {
      setError('Add the instructions this skill should apply.')
      setFieldError('content')
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
      setError(err instanceof Error ? err.message : 'Failed to create skill')
    } finally {
      setSubmitting(false)
    }
  }

  function updateField(field: keyof typeof emptyForm, value: string) {
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
        <div className="mb-4 flex items-center justify-between gap-3">
          <h2 id="create-skill-title" className={uiStyles.sectionTitle}>
            New Skill
          </h2>
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
          <div>
            <label htmlFor="skill-name" className={uiStyles.label}>
              Name
            </label>
            <input
              id="skill-name"
              ref={nameInputRef}
              value={form.name}
              onChange={(event) => updateField('name', event.target.value)}
              className={uiStyles.input}
              placeholder="frontend-review"
              aria-invalid={fieldError === 'name'}
              aria-describedby={
                fieldError === 'name' ? 'skill-name-help create-skill-error' : 'skill-name-help'
              }
              autoFocus
            />
            <p
              id="skill-name-help"
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              Use a short name people can recognize when selecting skills.
            </p>
          </div>

          <div>
            <label htmlFor="skill-description" className={uiStyles.label}>
              Description
            </label>
            <input
              id="skill-description"
              value={form.description}
              onChange={(event) => updateField('description', event.target.value)}
              className={uiStyles.input}
              placeholder="What this skill helps with"
              aria-describedby="skill-description-help"
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
              Trigger Pattern
            </label>
            <input
              id="skill-trigger"
              value={form.triggerPattern}
              onChange={(event) => updateField('triggerPattern', event.target.value)}
              className={cn(uiStyles.input, 'font-mono')}
              placeholder="frontend, review, release"
              aria-describedby="skill-trigger-help"
            />
            <p
              id="skill-trigger-help"
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              Optional. Add words that should suggest this skill.
            </p>
          </div>

          <div>
            <label htmlFor="skill-content" className={uiStyles.label}>
              Content
            </label>
            <textarea
              id="skill-content"
              ref={contentInputRef}
              value={form.content}
              onChange={(event) => updateField('content', event.target.value)}
              className={cn(
                'min-h-36 w-full resize-y rounded-[18px] border border-black/[0.08] bg-white px-3 py-2 font-mono text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70'
              )}
              placeholder="Check the user path first. List blockers before suggestions."
              aria-invalid={fieldError === 'content'}
              aria-describedby={
                fieldError === 'content'
                  ? 'skill-content-help create-skill-error'
                  : 'skill-content-help'
              }
            />
            <p
              id="skill-content-help"
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              Required. Write the instructions the agent should apply when this skill is selected.
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
