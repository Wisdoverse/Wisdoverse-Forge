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
            Platform API key created — copy it now, it won&apos;t be shown again
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
  const [submitAttempted, setSubmitAttempted] = useState(false)
  const nameInputId = 'platform-key-name'
  const statusId = 'platform-key-form-status'
  const errorId = 'platform-key-name-error'
  const trimmedName = name.trim()
  const isReady = Boolean(trimmedName)
  const visibleError =
    submitAttempted && !isReady ? 'Name this platform API key before creating it.' : null

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
    if (!isReady) {
      document.getElementById(nameInputId)?.focus()
      return
    }
    await onSave(trimmedName)
  }

  return (
    <form onSubmit={handleSubmit} noValidate className="mt-3">
      <div
        id={statusId}
        data-testid="platform-key-form-status"
        aria-live="polite"
        className={cn(
          'mb-3 rounded-card border px-3 py-2',
          isReady
            ? 'border-apple-green/25 bg-apple-green/10'
            : 'border-apple-blue/20 bg-apple-blue/[0.04]'
        )}
      >
        <p className="text-ui-button font-semibold text-foreground-light dark:text-foreground-dark">
          {isReady ? 'Ready to Create Key' : 'Next: Name the Platform Key'}
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {isReady
            ? 'Create this key, then copy it before closing the one-time key banner.'
            : 'Use a name that tells future admins where this key will be used.'}
        </p>
      </div>

      {visibleError && (
        <div className={cn(uiStyles.error, 'mb-3')} role="alert" aria-live="polite">
          {visibleError}
        </div>
      )}

      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto_auto] sm:items-end">
        <div className="min-w-0">
          <label htmlFor={nameInputId} className={uiStyles.label}>
            Key Name
          </label>
          <input
            id={nameInputId}
            name="platformKeyName"
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. CI job or integration…"
            autoFocus
            autoComplete="off"
            spellCheck={false}
            aria-invalid={visibleError !== null}
            aria-describedby={`${statusId}${visibleError ? ` ${errorId}` : ''}`}
            className={cn(uiStyles.input, 'min-w-0')}
          />
          {visibleError && (
            <p id={errorId} className="mt-1 text-ui-caption text-apple-red">
              {visibleError}
            </p>
          )}
        </div>
        <button
          type="button"
          onClick={onCancel}
          disabled={saving}
          className={uiStyles.secondaryButton}
        >
          Cancel
        </button>
        <button type="submit" disabled={saving} className={uiStyles.primaryButton}>
          {saving ? 'Creating…' : 'Create'}
        </button>
      </div>
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
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No platform API keys yet
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Create a platform key only when scripts or integrations need Forge API access
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
