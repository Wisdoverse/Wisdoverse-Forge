import { useEffect, useState, type FormEvent } from 'react'
import { CheckCircle2, KeyRound } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { ApiKeyRecord } from '@app/shared/api/legacy/settingsApi'
import { formatAccessDate } from './formatAccessDate'
import { platformKeyErrorMessage } from './platformKeyErrorMessage'

const ACCESS_KEY_EMPTY_STEPS = [
  'Create one only for a tool you trust.',
  'Name it after the exact tool or job that will use it.',
  'Copy the new key into a password manager before closing this message.',
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
  const removeWarningId = `automation-key-remove-warning-${apiKey.id}`

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
          {formatAccessDate(apiKey.createdAt, {
            missing: 'Refresh access keys to load created date',
            invalid: 'Refresh access keys to check created date',
          })}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {formatAccessDate(apiKey.lastUsedAt, {
            missing: 'Use this key from a trusted tool first',
            invalid: 'Refresh access keys to check last use',
          })}
        </span>
      </td>
      <td className={cn(uiStyles.tableCell, 'text-right')}>
        <button
          type="button"
          onClick={handleRevoke}
          aria-label={
            confirming
              ? `Confirm removing outside tool access key named ${apiKey.name}`
              : `Remove outside tool access key named ${apiKey.name}`
          }
          aria-describedby={confirming ? removeWarningId : undefined}
          className={confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton}
        >
          {confirming ? 'Remove now' : 'Remove'}
        </button>
        {confirming && (
          <p id={removeWarningId} className="ml-auto mt-1 max-w-48 text-ui-caption text-apple-red">
            Removing this key can stop {apiKey.name} from connecting to Forge.
          </p>
        )}
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
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0 flex-1">
          <p className="mb-1 text-ui-caption font-semibold">
            Outside tool access key created - save it now
          </p>
          <p className="mb-2 text-ui-caption text-apple-blue/80">
            This is the only time the full key is shown. Copy it into a password manager before
            choosing I saved it.
          </p>
          <code className="break-all font-mono text-ui-caption">{keyValue}</code>
        </div>
        <div className="flex gap-2 sm:shrink-0">
          <button
            type="button"
            onClick={handleCopy}
            className={cn(
              copied ? uiStyles.primaryButton : uiStyles.secondaryButton,
              'flex-1 sm:flex-none'
            )}
          >
            {copied ? 'Copied' : 'Copy key'}
          </button>
          <button
            type="button"
            onClick={onDismiss}
            className={cn(uiStyles.subtleButton, 'flex-1 sm:flex-none')}
          >
            I saved it
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
  const nameHelpId = 'platform-key-name-help'
  const nameErrorId = 'platform-key-name-error'
  const trimmedName = name.trim()
  const isReady = Boolean(trimmedName)
  const visibleError =
    submitAttempted && !isReady ? 'Name the tool that will use this access key first.' : null

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
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
      <label htmlFor={nameInputId} className={uiStyles.label}>
        Which tool will use this key?
      </label>
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
        <input
          id={nameInputId}
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. release tool"
          autoFocus
          aria-invalid={visibleError !== null}
          aria-describedby={`${nameHelpId}${visibleError ? ` ${nameErrorId}` : ''}`}
          className={cn(uiStyles.input, 'min-w-0 flex-1')}
        />
        <button
          type="button"
          onClick={onCancel}
          disabled={saving}
          className={cn(uiStyles.secondaryButton, 'w-full sm:w-auto')}
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={saving || !name.trim()}
          className={cn(uiStyles.primaryButton, 'w-full sm:w-auto')}
        >
          {saving ? 'Creating...' : 'Create access key'}
        </button>
      </div>
      <p
        id={nameHelpId}
        className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark"
      >
        Use a clear tool or job name. This makes it easy to remove the right key later.
      </p>
      {visibleError && (
        <p id={nameErrorId} role="alert" className="mt-1 text-ui-caption text-apple-red">
          {visibleError}
        </p>
      )}
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
    { label: 'Key preview' },
    { label: 'Created' },
    { label: 'Last used' },
    { label: '', className: 'w-20' },
  ]

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Outside tool access</h2>
          <p className={uiStyles.sectionDescription}>
            Let a trusted outside tool connect to Forge without asking a person to sign in.
          </p>
        </div>
        {!showForm && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <span>+</span>
            <span>Create access key</span>
          </button>
        )}
      </div>

      {/* New key banner */}
      {newKeyValue && (
        <NewKeyBanner keyValue={newKeyValue} onDismiss={() => setNewKeyValue(null)} />
      )}

      {/* Error */}
      {keysError && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {platformKeyErrorMessage(keysError)}
        </div>
      )}

      {/* Create form */}
      {showForm && (
        <CreateKeyForm onSave={handleCreate} onCancel={() => setShowForm(false)} saving={saving} />
      )}

      {/* Table */}
      {(keysLoading || apiKeys.length > 0 || !showForm) && (
        <div className={cn(uiStyles.card, 'mt-3 overflow-x-auto')}>
          {keysLoading && apiKeys.length === 0 ? (
            <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
              Loading access keys…
            </div>
          ) : apiKeys.length === 0 ? (
            <PlatformKeyEmptyState onCreate={() => setShowForm(true)} />
          ) : (
            <table className={uiStyles.table} aria-label="Outside tool access keys">
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
      )}
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
              Add a key only for a trusted outside tool
            </h3>
            <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
              Use this only when a trusted outside tool needs to connect without a person signing
              in.
            </p>
          </div>
        </div>
        <div className="grid gap-2 sm:grid-cols-3">
          {ACCESS_KEY_EMPTY_STEPS.map((step) => (
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
          <span>Create access key</span>
        </button>
      </div>
    </section>
  )
}
