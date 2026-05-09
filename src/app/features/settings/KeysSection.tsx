import { useEffect, useState } from 'react'
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
            API key created — copy it now, it won&apos;t be shown again
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

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!name.trim()) return
    await onSave(name.trim())
  }

  return (
    <form onSubmit={handleSubmit} className="flex items-center gap-2 mt-3">
      <input
        type="text"
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Key name (e.g. CI/CD pipeline)"
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
          <h2 className={uiStyles.sectionTitle}>API Keys</h2>
          <p className={uiStyles.sectionDescription}>Manage API keys for programmatic access</p>
        </div>
        {!showForm && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <span>+</span>
            <span>Create Key</span>
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
            Loading keys...
          </div>
        ) : apiKeys.length === 0 ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No API keys yet
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Create a key to enable programmatic access
            </p>
          </div>
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
