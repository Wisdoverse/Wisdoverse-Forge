import { useEffect, useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { UserSshKey } from '@app/shared/api/legacy/AgentAPI'

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

// ============================================================================
// SSH Key Row
// ============================================================================

interface SshKeyRowProps {
  sshKey: UserSshKey
  onDelete: (id: string) => void
}

function SshKeyRow({ sshKey, onDelete }: SshKeyRowProps) {
  const [confirming, setConfirming] = useState(false)

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
          {sshKey.keyType}
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
              ? `Confirm removing ${sshKey.label} SSH key`
              : `Remove ${sshKey.label} SSH key`
          }
          className={confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton}
        >
          {confirming ? 'Remove key?' : 'Remove'}
        </button>
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
  const publicKeyInputId = 'ssh-key-public-key'
  const statusId = 'ssh-key-form-status'
  const errorId = 'ssh-key-form-error'
  const trimmedLabel = label.trim()
  const trimmedPublicKey = publicKey.trim()
  const missingField = !trimmedLabel ? 'label' : !trimmedPublicKey ? 'publicKey' : null
  const isReady = missingField === null
  const visibleError =
    submitAttempted && missingField === 'label'
      ? 'Name this SSH key before saving it.'
      : submitAttempted && missingField === 'publicKey'
        ? 'Paste the public SSH key before saving it.'
        : null
  const readinessTitle =
    missingField === 'label'
      ? 'Next: Name the SSH Key'
      : missingField === 'publicKey'
        ? 'Next: Paste the Public Key'
        : 'Ready to Add SSH Key'
  const readinessDetail =
    missingField === 'label'
      ? 'Use a name that helps you recognize where this key lives, such as Laptop or CI runner.'
      : missingField === 'publicKey'
        ? 'Paste the public key only. It usually starts with ssh-ed25519, ssh-rsa, or ecdsa-sha2.'
        : 'Add this key, then use a small agent task to confirm repository access.'

  async function handleSubmit(e: React.FormEvent) {
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
      <div
        id={statusId}
        data-testid="ssh-key-form-status"
        aria-live="polite"
        className={cn(
          'mb-3 rounded-card border px-3 py-2',
          isReady
            ? 'border-apple-green/25 bg-apple-green/10'
            : 'border-apple-blue/20 bg-apple-blue/[0.04]'
        )}
      >
        <p className="text-ui-button font-semibold text-foreground-light dark:text-foreground-dark">
          {readinessTitle}
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {readinessDetail}
        </p>
      </div>

      {visibleError && (
        <div className={cn(uiStyles.error, 'mb-3')} role="alert" aria-live="polite">
          {visibleError}
        </div>
      )}

      <div className="flex flex-col gap-3 mb-3">
        <div>
          <label htmlFor="ssh-key-label" className={uiStyles.label}>
            Key name <span className="text-red-500">*</span>
          </label>
          <input
            id="ssh-key-label"
            type="text"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="e.g. GitHub deploy key"
            aria-describedby="ssh-key-label-help"
            autoFocus
            autoComplete="off"
            spellCheck={false}
            aria-invalid={visibleError !== null && missingField === 'label'}
            aria-describedby={`${statusId}${visibleError !== null && missingField === 'label' ? ` ${errorId}` : ''}`}
            className={uiStyles.input}
          />
          <p
            id="ssh-key-label-help"
            className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Use a name that tells your team where this key is used.
          </p>
        </div>

        <div>
          <label htmlFor="ssh-key-public" className={uiStyles.label}>
            Public key text <span className="text-red-500">*</span>
          </label>
          <textarea
            id="ssh-key-public"
            value={publicKey}
            onChange={(e) => setPublicKey(e.target.value)}
            placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA... user@host"
            aria-describedby="ssh-key-public-help"
            required
            rows={6}
            className={cn(
              'w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-3 py-2 font-mono text-ui-caption text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70'
            )}
          />
          <p
            id="ssh-key-public-help"
            className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Paste only the public key that starts with ssh-ed25519 or ssh-rsa. Never paste a private
            key.
          </p>
        </div>
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
        <button
          type="submit"
          disabled={saving || !label.trim() || !publicKey.trim()}
          className={uiStyles.primaryButton}
        >
          {saving ? 'Saving...' : 'Save SSH key'}
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
    { label: 'Fingerprint' },
    { label: 'Key type' },
    { label: 'Added on' },
    { label: '', className: 'w-20' },
  ]

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Repository SSH keys</h2>
          <p className={uiStyles.sectionDescription}>
            Add public keys that let agents access private repositories without a password.
          </p>
        </div>
        {!showForm && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <span>+</span>
            <span>Add SSH key</span>
          </button>
        )}
      </div>

      {/* Error */}
      {sshKeysError && <div className={uiStyles.error}>{sshKeysError}</div>}

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {sshKeysLoading && sshKeys.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading repository SSH keys...
          </div>
        ) : sshKeys.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No repository SSH keys yet
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Add a public key before assigning private repository work that uses SSH access.
            </p>
          </div>
        ) : (
          <>
            {sshKeys.length > 0 && (
              <table className={uiStyles.table} aria-label="Repository SSH keys">
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
