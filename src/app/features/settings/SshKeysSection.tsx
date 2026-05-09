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
          className={confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton}
        >
          {confirming ? 'Confirm?' : 'Delete'}
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

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!label.trim() || !publicKey.trim()) return
    await onSave(label.trim(), publicKey.trim())
  }

  return (
    <form
      onSubmit={handleSubmit}
      className={cn(
        'border-t border-black/[0.06] p-4 dark:border-white/[0.08]',
        'bg-black/[0.015] dark:bg-white/[0.025]'
      )}
    >
      <div className="flex flex-col gap-3 mb-3">
        {/* Label */}
        <div>
          <label className={uiStyles.label}>
            Label <span className="text-red-500">*</span>
          </label>
          <input
            type="text"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="e.g. My Laptop Key"
            autoFocus
            required
            className={uiStyles.input}
          />
        </div>

        {/* Public Key */}
        <div>
          <label className={uiStyles.label}>
            Public Key <span className="text-red-500">*</span>
          </label>
          <textarea
            value={publicKey}
            onChange={(e) => setPublicKey(e.target.value)}
            placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA... user@host"
            required
            rows={6}
            className={cn(
              'w-full resize-none rounded-[18px] border border-black/[0.08] bg-white px-3 py-2 font-mono text-ui-caption text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70'
            )}
          />
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Paste the public key used by agent containers for git operations.
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
          {saving ? 'Saving...' : 'Add SSH Key'}
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
    { label: 'Label' },
    { label: 'Fingerprint' },
    { label: 'Type' },
    { label: 'Added' },
    { label: '', className: 'w-20' },
  ]

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>SSH Keys</h2>
          <p className={uiStyles.sectionDescription}>SSH keys used by agents for git operations</p>
        </div>
        {!showForm && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <span>+</span>
            <span>Add Key</span>
          </button>
        )}
      </div>

      {/* Error */}
      {sshKeysError && <div className={uiStyles.error}>{sshKeysError}</div>}

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {sshKeysLoading && sshKeys.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading SSH keys...
          </div>
        ) : sshKeys.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No SSH keys yet
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Add an SSH key to enable authenticated git operations
            </p>
          </div>
        ) : (
          <>
            {sshKeys.length > 0 && (
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
