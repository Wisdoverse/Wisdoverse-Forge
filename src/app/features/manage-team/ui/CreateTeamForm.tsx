import { useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { slugifyName } from '@app/shared/lib/slugify'

interface CreateTeamFormProps {
  onSave: (name: string) => Promise<void>
  onCancel: () => void
  saving: boolean
}

export function CreateTeamForm({ onSave, onCancel, saving }: CreateTeamFormProps) {
  const [name, setName] = useState('')

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!name.trim()) return
    await onSave(name.trim())
  }

  return (
    <form
      onSubmit={handleSubmit}
      className={cn(
        'border-t border-black/[0.06] p-4 dark:border-white/[0.08]',
        'bg-black/[0.015] dark:bg-white/[0.025]'
      )}
    >
      <div className="mb-3">
        <label className={uiStyles.label}>Team Name</label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. Frontend"
          autoFocus
          required
          className={uiStyles.input}
        />
        {name.trim() && (
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Slug: {slugifyName(name)}
          </p>
        )}
      </div>
      <div className="flex gap-2 justify-end">
        <button
          type="button"
          onClick={onCancel}
          disabled={saving}
          className={uiStyles.secondaryButton}
        >
          Cancel
        </button>
        <button type="submit" disabled={saving || !name.trim()} className={uiStyles.primaryButton}>
          {saving ? 'Creating...' : 'Create Team'}
        </button>
      </div>
    </form>
  )
}
