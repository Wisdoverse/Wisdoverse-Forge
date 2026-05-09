import { useEffect, useState } from 'react'
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
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    if (!open) return
    setForm(emptyForm)
    setError(null)

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

        {error && <div className={uiStyles.error}>{error}</div>}

        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div>
            <label htmlFor="skill-name" className={uiStyles.label}>
              Name
            </label>
            <input
              id="skill-name"
              value={form.name}
              onChange={(event) => setForm((current) => ({ ...current, name: event.target.value }))}
              className={uiStyles.input}
              placeholder="e.g. frontend-review"
              autoFocus
            />
          </div>

          <div>
            <label htmlFor="skill-description" className={uiStyles.label}>
              Description
            </label>
            <input
              id="skill-description"
              value={form.description}
              onChange={(event) =>
                setForm((current) => ({ ...current, description: event.target.value }))
              }
              className={uiStyles.input}
              placeholder="What this skill helps with"
            />
          </div>

          <div>
            <label htmlFor="skill-trigger" className={uiStyles.label}>
              Trigger Pattern
            </label>
            <input
              id="skill-trigger"
              value={form.triggerPattern}
              onChange={(event) =>
                setForm((current) => ({ ...current, triggerPattern: event.target.value }))
              }
              className={cn(uiStyles.input, 'font-mono')}
              placeholder="Optional keyword or regex"
            />
          </div>

          <div>
            <label htmlFor="skill-content" className={uiStyles.label}>
              Content
            </label>
            <textarea
              id="skill-content"
              value={form.content}
              onChange={(event) =>
                setForm((current) => ({ ...current, content: event.target.value }))
              }
              className={cn(
                'min-h-36 w-full resize-y rounded-[18px] border border-black/[0.08] bg-white px-3 py-2 font-mono text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70'
              )}
              placeholder="Instructions the agent should apply when this skill is selected"
            />
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
