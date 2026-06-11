import { useEffect, useState, type FormEvent } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { UserSshKey } from '@app/entities/agent'
import { sshKeysErrorMessage } from './sshKeysErrorMessage'

// ============================================================================
// Helpers
// ============================================================================

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}

function describeKeyType(keyType: string): string {
  if (keyType === 'ssh-ed25519') return 'Modern SSH key'
  if (keyType === 'ssh-rsa') return 'RSA SSH key'
  return keyType
}

const SSH_KEY_SETUP_STEPS = [
  { label: 'Name this access', value: 'Use a device, team, or repository name.' },
  {
    label: 'Paste the shareable line',
    value: 'Copy only the one-line key that starts with ssh-ed25519 or ssh-rsa.',
  },
  {
    label: 'Keep the secret key private',
    value: 'Never paste anything that says BEGIN PRIVATE KEY.',
  },
]

// ============================================================================
// SSH Key Row
// ============================================================================

interface SshKeyRowProps {
  sshKey: UserSshKey
  onDelete: (id: string) => void
}

function SshKeyRow({ sshKey, onDelete }: SshKeyRowProps) {
  const [confirming, setConfirming] = useState(false)
  const removeWarningId = `ssh-key-remove-warning-${sshKey.id}`

  function handleDelete() {
    if (!confirming) {
      setConfirming(true)
      return
    }
    onDelete(sshKey.id)
    setConfirming(false)
  }

  return (
    <tr className={uiStyles.row}>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-body text-foreground-light dark:text-foreground-dark">
          {sshKey.label}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="font-mono text-ui-caption text-secondary-light dark:text-secondary-dark">
          {sshKey.fingerprint}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {describeKeyType(sshKey.keyType)}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {formatDate(sshKey.createdAt)}
        </span>
      </td>
      <td className={cn(uiStyles.tableCell, 'text-right')}>
        <button
          type="button"
          onClick={handleDelete}
          aria-label={
            confirming
              ? `Confirm removing ${sshKey.label} repository SSH access`
              : `Remove ${sshKey.label} repository SSH access`
          }
          aria-describedby={confirming ? removeWarningId : undefined}
          className={confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton}
        >
          {confirming ? 'Remove access now' : 'Remove'}
        </button>
        {confirming && (
          <p id={removeWarningId} className="ml-auto mt-1 max-w-44 text-ui-caption text-apple-red">
            Removing this access can block agents that use code repositories that are not public.
          </p>
        )}
      </td>
    </tr>
  )
}

// ============================================================================
// Add SSH Key Form
// ============================================================================

interface AddSshKeyFormProps {
  onSave: (label: string, publicKey: string) => Promise<void>
  onCancel: () => void
  saving: boolean
}

function AddSshKeyForm({ onSave, onCancel, saving }: AddSshKeyFormProps) {
  const [label, setLabel] = useState('')
  const [publicKey, setPublicKey] = useState('')
  const [submitAttempted, setSubmitAttempted] = useState(false)
  const labelInputId = 'ssh-key-label'
  const labelHelpId = 'ssh-key-label-help'
  const publicKeyInputId = 'ssh-public-key'
  const publicKeyHelpId = 'ssh-public-key-help'
  const publicKeySafetyId = 'ssh-public-key-safety'
  const errorId = 'ssh-key-form-error'
  const trimmedLabel = label.trim()
  const trimmedPublicKey = publicKey.trim()
  const missingField = !trimmedLabel ? 'label' : !trimmedPublicKey ? 'publicKey' : null
  const isReady = missingField === null
  const visibleError =
    submitAttempted && missingField === 'label'
      ? 'Add a name your team will recognize before saving.'
      : submitAttempted && missingField === 'publicKey'
        ? 'Paste the shareable SSH line before saving.'
        : null

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
    if (!isReady) {
      document.getElementById(missingField === 'label' ? labelInputId : publicKeyInputId)?.focus()
      return
    }
    await onSave(trimmedLabel, trimmedPublicKey)
  }

  return (
    <form
      onSubmit={handleSubmit}
      noValidate
      className={cn(
        'border-t border-black/[0.06] p-4 dark:border-white/[0.08]',
        'bg-black/[0.015] dark:bg-white/[0.025]'
      )}
    >
      <div className="mb-3 rounded-lg border border-black/[0.06] bg-white px-3 py-2.5 dark:border-white/[0.08] dark:bg-black/20">
        <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          Add repository SSH access
        </div>
        <div className="mt-2 grid gap-1.5 sm:grid-cols-3">
          {SSH_KEY_SETUP_STEPS.map((step) => (
            <div
              key={step.label}
              className="min-w-0 rounded-md bg-black/[0.025] px-2 py-1.5 dark:bg-white/[0.04]"
            >
              <span className="block text-[10px] font-medium text-secondary-light dark:text-secondary-dark">
                {step.label}
              </span>
              <span className="mt-0.5 block text-ui-caption text-foreground-light dark:text-foreground-dark">
                {step.value}
              </span>
            </div>
          ))}
        </div>
      </div>

      <div className="flex flex-col gap-3 mb-3">
        <div>
          <label htmlFor="ssh-key-label" className={uiStyles.label}>
            Name for this access <span className="text-red-500">*</span>
          </label>
          <p
            id={labelHelpId}
            className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Use a device, team, or repository name, for example Work laptop.
          </p>
          <input
            id={labelInputId}
            type="text"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="e.g. Work laptop"
            autoFocus
            autoComplete="off"
            spellCheck={false}
            aria-invalid={visibleError !== null && missingField === 'label'}
            aria-describedby={`${labelHelpId}${visibleError !== null && missingField === 'label' ? ` ${errorId}` : ''}`}
            className={uiStyles.input}
          />
        </div>

        <div>
          <label htmlFor="ssh-public-key" className={uiStyles.label}>
            Shareable SSH line <span className="text-red-500">*</span>
          </label>
          <p
            id={publicKeyHelpId}
            className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Copy the one-line shareable SSH key from the tool that generated it.
          </p>
          <textarea
            id={publicKeyInputId}
            value={publicKey}
            onChange={(e) => setPublicKey(e.target.value)}
            placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA... dev@example.com"
            required
            rows={6}
            className={cn(
              'w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-3 py-2 font-mono text-ui-caption text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70'
            )}
            aria-describedby={`${publicKeyHelpId} ${publicKeySafetyId}${visibleError !== null && missingField === 'publicKey' ? ` ${errorId}` : ''}`}
          />
          <p
            id={publicKeySafetyId}
            className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            It starts with ssh-ed25519 or ssh-rsa. Never paste anything that says BEGIN PRIVATE KEY.
          </p>
        </div>
      </div>
      {visibleError && (
        <p id={errorId} role="alert" className="mb-3 text-ui-caption text-apple-red">
          {visibleError}
        </p>
      )}

      <div className="flex gap-2 justify-end">
        <button
          type="button"
          onClick={onCancel}
          disabled={saving}
          className={uiStyles.secondaryButton}
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={saving || !label.trim() || !publicKey.trim()}
          className={uiStyles.primaryButton}
        >
          {saving ? 'Saving...' : 'Save SSH access'}
        </button>
      </div>
    </form>
  )
}

// ============================================================================
// SshKeysSection
// ============================================================================

export function SshKeysSection() {
  const { sshKeys, sshKeysLoading, sshKeysError, loadSshKeys, createSshKey, deleteSshKey } =
    useSettingsStore()
  const [showForm, setShowForm] = useState(false)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    void loadSshKeys()
  }, [loadSshKeys])

  async function handleSave(label: string, publicKey: string) {
    setSaving(true)
    const ok = await createSshKey(label, publicKey)
    setSaving(false)
    if (ok) setShowForm(false)
  }

  async function handleDelete(id: string) {
    await deleteSshKey(id)
  }

  const tableHeaders: { label: string; className?: string }[] = [
    { label: 'Key name' },
    { label: 'Saved key ID' },
    { label: 'Key kind' },
    { label: 'Added on' },
    { label: '', className: 'w-20' },
  ]

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Repository SSH access</h2>
          <p className={uiStyles.sectionDescription}>
            Let agents open code repositories that are not public and use addresses starting with
            git@.
          </p>
        </div>
        {!showForm && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <span>+</span>
            <span>Add SSH access</span>
          </button>
        )}
      </div>

      {/* Error */}
      {sshKeysError && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {sshKeysErrorMessage(sshKeysError)}
        </div>
      )}

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {sshKeysLoading && sshKeys.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading repository SSH access...
          </div>
        ) : sshKeys.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No repository SSH access yet
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Code repository addresses that start with git@ need this. If the address starts with
              https://, use repository access instead.
            </p>
          </div>
        ) : (
          <>
            {sshKeys.length > 0 && (
              <table className={uiStyles.table} aria-label="Repository SSH access">
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
                  {sshKeys.map((key: UserSshKey) => (
                    <SshKeyRow key={key.id} sshKey={key} onDelete={handleDelete} />
                  ))}
                </tbody>
              </table>
            )}
          </>
        )}

        {/* Add form */}
        {showForm && (
          <AddSshKeyForm onSave={handleSave} onCancel={() => setShowForm(false)} saving={saving} />
        )}
      </div>
    </div>
  )
}
