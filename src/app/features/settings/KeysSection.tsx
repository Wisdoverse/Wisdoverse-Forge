import { useEffect, useState, type FormEvent } from 'react'
import { CheckCircle2, KeyRound } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { ApiKeyRecord } from '@app/shared/api/legacy/settingsApi'

// ============================================================================
// Helpers
// ============================================================================

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '—'
  return new Date(dateStr).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}

const API_KEY_EMPTY_STEPS = [
  'Create a key only for a trusted script, CI job, or integration.',
  'Use a name that tells the team where the key will live.',
  'Copy the new key into a password manager or CI secret before closing the banner.',
]

// ============================================================================
// Key Row
// ============================================================================

interface KeyRowProps {
  apiKey: ApiKeyRecord
  onRevoke: (id: string) => void
}

function KeyRow({ apiKey, onRevoke }: KeyRowProps) {
  const [confirming, setConfirming] = useState(false)

  function handleRevoke() {
    if (!confirming) {
      setConfirming(true)
      return
    }
    onRevoke(apiKey.id)
    setConfirming(false)
  }

  return (
    <tr className={uiStyles.row}>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-body text-foreground-light dark:text-foreground-dark">
          {apiKey.name}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="font-mono text-ui-caption text-secondary-light dark:text-secondary-dark">
          {apiKey.keyPrefix}••••••••
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {formatDate(apiKey.createdAt)}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {formatDate(apiKey.lastUsedAt)}
        </span>
      </td>
      <td className={cn(uiStyles.tableCell, 'text-right')}>
        <button
          type="button"
          onClick={handleRevoke}
          className={confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton}
        >
          {confirming ? 'Confirm?' : 'Revoke'}
        </button>
      </td>
    </tr>
  )
}

// ============================================================================
// New Key Banner (shown once after creation)
// ============================================================================

interface NewKeyBannerProps {
  keyValue: string
  onDismiss: () => void
}

function NewKeyBanner({ keyValue, onDismiss }: NewKeyBannerProps) {
  const [copied, setCopied] = useState(false)

  function handleCopy() {
    void navigator.clipboard.writeText(keyValue).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }

  return (
    <div
      className={cn(
        'mb-4 rounded-card border border-apple-blue/20 bg-apple-blue/10 p-4 text-apple-blue'
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <p className="mb-1 text-ui-caption font-semibold">
            Platform API key created — copy it now, it won&apos;t be shown again
          </p>
          <p className="mb-2 text-ui-caption text-apple-blue/80">
            Save it in your password manager or CI secret store before dismissing this banner.
          </p>
          <code className="break-all font-mono text-ui-caption">{keyValue}</code>
        </div>
        <div className="flex shrink-0 gap-2">
          <button
            type="button"
            onClick={handleCopy}
            className={copied ? uiStyles.primaryButton : uiStyles.secondaryButton}
          >
            {copied ? 'Copied!' : 'Copy'}
          </button>
          <button type="button" onClick={onDismiss} className={uiStyles.subtleButton}>
            Dismiss
          </button>
        </div>
      </div>
    </div>
  )
}

// ============================================================================
// Create Key Form
// ============================================================================

interface CreateKeyFormProps {
  onSave: (name: string) => Promise<void>
  onCancel: () => void
  saving: boolean
}

function CreateKeyForm({ onSave, onCancel, saving }: CreateKeyFormProps) {
  const [name, setName] = useState('')
  const nameInputId = 'platform-api-key-name'
  const trimmedName = name.trim()
  const isReady = Boolean(trimmedName)

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    if (!isReady) {
      document.getElementById(nameInputId)?.focus()
      return
    }
    await onSave(trimmedName)
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="mt-3 rounded-card border border-black/[0.08] bg-white p-3 dark:border-white/[0.1] dark:bg-[#2c2c2e]"
    >
      <label htmlFor="platform-key-name" className={uiStyles.label}>
        Key name
      </label>
      <div className="flex items-center gap-2">
        <input
          id="platform-key-name"
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. production deploy pipeline"
          autoFocus
          className={cn(uiStyles.input, 'min-w-0 flex-1')}
        />
        <button
          type="button"
          onClick={onCancel}
          disabled={saving}
          className={uiStyles.secondaryButton}
        >
          Cancel
        </button>
        <button type="submit" disabled={saving || !name.trim()} className={uiStyles.primaryButton}>
          {saving ? 'Creating...' : 'Create'}
        </button>
      </div>
      <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
        Name the exact place this key will be used so it is easy to revoke later.
      </p>
    </form>
  )
}

// ============================================================================
// KeysSection
// ============================================================================

export function KeysSection() {
  const { apiKeys, keysLoading, keysError, loadApiKeys, createApiKey, revokeApiKey } =
    useSettingsStore()
  const [showForm, setShowForm] = useState(false)
  const [saving, setSaving] = useState(false)
  const [newKeyValue, setNewKeyValue] = useState<string | null>(null)

  useEffect(() => {
    void loadApiKeys()
  }, [loadApiKeys])

  async function handleCreate(name: string) {
    setSaving(true)
    const result = await createApiKey(name)
    setSaving(false)
    if (result) {
      setNewKeyValue(result.key)
      setShowForm(false)
    }
  }

  async function handleRevoke(id: string) {
    await revokeApiKey(id)
  }

  const tableHeaders: { label: string; className?: string }[] = [
    { label: 'Name' },
    { label: 'Key' },
    { label: 'Created' },
    { label: 'Last Used' },
    { label: '', className: 'w-20' },
  ]

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Platform API Keys</h2>
          <p className={uiStyles.sectionDescription}>
            Create tokens for scripts, CI jobs, and external integrations that call Forge APIs
          </p>
        </div>
        {!showForm && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <span>+</span>
            <span>Create Platform Key</span>
          </button>
        )}
      </div>

      {/* New key banner */}
      {newKeyValue && (
        <NewKeyBanner keyValue={newKeyValue} onDismiss={() => setNewKeyValue(null)} />
      )}

      {/* Error */}
      {keysError && <div className={uiStyles.error}>{keysError}</div>}

      {/* Create form */}
      {showForm && (
        <CreateKeyForm onSave={handleCreate} onCancel={() => setShowForm(false)} saving={saving} />
      )}

      {/* Table */}
      <div className={cn(uiStyles.card, 'mt-3 overflow-x-auto')}>
        {keysLoading && apiKeys.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading keys…
          </div>
        ) : apiKeys.length === 0 ? (
          <PlatformKeyEmptyState onCreate={() => setShowForm(true)} />
        ) : (
          <table className={uiStyles.table}>
            <thead className={uiStyles.tableHead}>
              <tr>
                {tableHeaders.map((h) => (
                  <th key={h.label} className={cn(uiStyles.tableHeaderCell, h.className)}>
                    {h.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {apiKeys.map((key: ApiKeyRecord) => (
                <KeyRow key={key.id} apiKey={key} onRevoke={handleRevoke} />
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}

function PlatformKeyEmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <section
      data-testid="platform-key-empty-state"
      className="px-4 py-6"
      aria-labelledby="platform-key-empty-title"
    >
      <div className="mx-auto flex max-w-2xl flex-col gap-3">
        <div className="flex items-start gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-apple-blue/10 text-apple-blue">
            <KeyRound size={17} strokeWidth={2.15} aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <h3
              id="platform-key-empty-title"
              className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark"
            >
              No platform API keys yet
            </h3>
            <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
              Create one only when another tool needs to call Forge without a signed-in user.
            </p>
          </div>
        </div>
        <div className="grid gap-2 sm:grid-cols-3">
          {API_KEY_EMPTY_STEPS.map((step) => (
            <div
              key={step}
              className="flex min-h-16 items-start gap-2 rounded-lg bg-black/[0.025] px-3 py-2 dark:bg-white/[0.05]"
            >
              <CheckCircle2
                size={14}
                strokeWidth={2.15}
                className="mt-0.5 shrink-0 text-apple-green"
                aria-hidden="true"
              />
              <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                {step}
              </span>
            </div>
          ))}
        </div>
        <button type="button" onClick={onCreate} className={cn(uiStyles.primaryButton, 'w-fit')}>
          <span>+</span>
          <span>Create Platform Key</span>
        </button>
      </div>
    </section>
  )
}
