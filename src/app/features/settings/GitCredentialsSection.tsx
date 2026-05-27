import { useEffect, useState } from 'react'
import { AlertTriangle, CheckCircle2 } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { GitCredential, GitProvider } from '@app/shared/api/legacy/AgentAPI'

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

const PROVIDER_LABELS: Record<GitProvider, string> = {
  github: 'GitHub',
  gitlab: 'GitLab',
}

interface CredentialFormReadiness {
  ready: boolean
  title: string
  detail: string
  error: string | null
  fieldId: string | null
}

function credentialFormReadiness({
  token,
  tokenInputId,
}: {
  token: string
  tokenInputId: string
}): CredentialFormReadiness {
  if (!token.trim()) {
    return {
      ready: false,
      title: 'Next: Paste Access Token',
      detail: 'Paste a token from GitHub or GitLab so agents can clone and push repositories.',
      error: 'Paste an access token before saving this credential.',
      fieldId: tokenInputId,
    }
  }

  return {
    ready: true,
    title: 'Ready to Save',
    detail: 'Save this token, then use a small agent task to confirm repository access.',
    error: null,
    fieldId: null,
  }
}

// ============================================================================
// Credential Row
// ============================================================================

interface CredentialRowProps {
  credential: GitCredential
  onDelete: (id: string) => void
}

function CredentialRow({ credential, onDelete }: CredentialRowProps) {
  const [confirming, setConfirming] = useState(false)

  function handleDelete() {
    if (!confirming) {
      setConfirming(true)
      return
    }
    onDelete(credential.id)
    setConfirming(false)
  }

  return (
    <tr className={uiStyles.row}>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-body text-foreground-light dark:text-foreground-dark">
          {PROVIDER_LABELS[credential.provider]}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {credential.host ?? 'Default cloud address'}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {formatDate(credential.createdAt)}
        </span>
      </td>
      <td className={cn(uiStyles.tableCell, 'text-right')}>
        <button
          type="button"
          onClick={handleDelete}
          aria-label={
            confirming
              ? `Confirm removing ${PROVIDER_LABELS[credential.provider]} repository token`
              : `Remove ${PROVIDER_LABELS[credential.provider]} repository token`
          }
          className={confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton}
        >
          {confirming ? 'Remove token?' : 'Remove'}
        </button>
      </td>
    </tr>
  )
}

// ============================================================================
// Add Credential Form
// ============================================================================

interface AddCredentialFormProps {
  existingProviders: GitProvider[]
  onSave: (provider: GitProvider, token: string, host?: string) => Promise<void>
  onCancel: () => void
  saving: boolean
}

interface AddCredentialFormState {
  provider: GitProvider
  token: string
  host: string
}

const DEFAULT_FORM: AddCredentialFormState = {
  provider: 'github',
  token: '',
  host: '',
}

function AddCredentialForm({
  existingProviders,
  onSave,
  onCancel,
  saving,
}: AddCredentialFormProps) {
  const [form, setForm] = useState<AddCredentialFormState>(DEFAULT_FORM)
  const [submitAttempted, setSubmitAttempted] = useState(false)
  const providerInputId = 'git-credential-provider'
  const tokenInputId = 'git-credential-token'
  const hostInputId = 'git-credential-host'
  const formStatusId = 'git-credential-form-status'
  const tokenErrorId = 'git-credential-token-error'
  const readiness = credentialFormReadiness({ token: form.token, tokenInputId })
  const visibleError = submitAttempted && !readiness.ready ? readiness.error : null

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
    if (!readiness.ready) {
      if (readiness.fieldId) document.getElementById(readiness.fieldId)?.focus()
      return
    }
    await onSave(form.provider, form.token.trim(), form.host.trim() || undefined)
  }

  const availableProviders: GitProvider[] = (['github', 'gitlab'] as GitProvider[]).filter(
    (p) => !existingProviders.includes(p) || existingProviders.includes(form.provider)
  )

  return (
    <form
      onSubmit={handleSubmit}
      className={cn(
        'border-t border-black/[0.06] p-4 dark:border-white/[0.08]',
        'bg-black/[0.015] dark:bg-white/[0.025]'
      )}
      noValidate
    >
      <div
        id={formStatusId}
        data-testid="git-credential-form-status"
        aria-live="polite"
        className={cn(
          'mb-3 rounded-lg border px-3 py-2',
          readiness.ready
            ? 'border-apple-green/25 bg-apple-green/10'
            : 'border-apple-blue/20 bg-apple-blue/[0.04]'
        )}
      >
        <div className="flex items-center gap-2">
          {readiness.ready ? (
            <CheckCircle2
              size={16}
              strokeWidth={2.25}
              className="shrink-0 text-apple-green"
              aria-hidden="true"
            />
          ) : (
            <AlertTriangle
              size={16}
              strokeWidth={2.25}
              className="shrink-0 text-apple-blue"
              aria-hidden="true"
            />
          )}
          <p className="text-ui-button font-semibold text-foreground-light dark:text-foreground-dark">
            {readiness.title}
          </p>
        </div>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {readiness.detail}
        </p>
      </div>

      {visibleError && (
        <div className={cn(uiStyles.error, 'mb-3')} role="alert" aria-live="polite">
          {visibleError}
        </div>
      )}

      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div>
          <label htmlFor="git-credential-provider" className={uiStyles.label}>
            Git provider
          </label>
          <select
            id="git-credential-provider"
            value={form.provider}
            onChange={(e) => setForm({ ...form, provider: e.target.value as GitProvider })}
            className={cn(uiStyles.select, 'w-full')}
          >
            {availableProviders.map((p) => (
              <option key={p} value={p}>
                {PROVIDER_LABELS[p]}
              </option>
            ))}
          </select>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Choose where the repository is hosted.
          </p>
        </div>

        <div>
          <label htmlFor="git-credential-token" className={uiStyles.label}>
            Access token <span className="text-red-500">*</span>
          </label>
          <input
            id="git-credential-token"
            type="password"
            name="token"
            value={form.token}
            onChange={(e) => setForm({ ...form, token: e.target.value })}
            placeholder="Paste a repository access token"
            aria-describedby="git-credential-token-help"
            required
            className={uiStyles.input}
          />
          <p
            id="git-credential-token-help"
            className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Paste a token that can read the repositories your agents need. It will not be shown
            again after saving.
          </p>
        </div>

        <div className="sm:col-span-2">
          <label htmlFor="git-credential-host" className={uiStyles.label}>
            Self-hosted Git address{' '}
            <span className="text-secondary-light dark:text-secondary-dark font-normal">
              (optional)
            </span>
          </label>
          <input
            id="git-credential-host"
            type="text"
            name="host"
            value={form.host}
            onChange={(e) => setForm({ ...form, host: e.target.value })}
            placeholder="e.g. gitlab.company.com"
            aria-describedby="git-credential-host-help"
            className={uiStyles.input}
          />
          <p
            id="git-credential-host-help"
            className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Leave blank for github.com or gitlab.com.
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
          disabled={saving || !form.token.trim()}
          className={uiStyles.primaryButton}
        >
          {saving ? 'Saving...' : 'Save token'}
        </button>
      </div>
    </form>
  )
}

// ============================================================================
// GitCredentialsSection
// ============================================================================

export function GitCredentialsSection() {
  const {
    gitCredentials,
    gitCredentialsLoading,
    gitCredentialsError,
    loadGitCredentials,
    saveGitCredential,
    deleteGitCredential,
  } = useSettingsStore()
  const [showForm, setShowForm] = useState(false)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    void loadGitCredentials()
  }, [loadGitCredentials])

  async function handleSave(provider: GitProvider, token: string, host?: string) {
    setSaving(true)
    const ok = await saveGitCredential(provider, token, host)
    setSaving(false)
    if (ok) setShowForm(false)
  }

  async function handleDelete(id: string) {
    await deleteGitCredential(id)
  }

  const existingProviders = gitCredentials.map((c) => c.provider)
  const canAddMore = existingProviders.length < 2

  const tableHeaders: { label: string; className?: string }[] = [
    { label: 'Git provider' },
    { label: 'Address' },
    { label: 'Added on' },
    { label: '', className: 'w-20' },
  ]

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Repository access tokens</h2>
          <p className={uiStyles.sectionDescription}>
            Connect GitHub or GitLab so agents can clone and update repositories when a task needs
            code access.
          </p>
        </div>
        {!showForm && canAddMore && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <span>+</span>
            <span>Add repository token</span>
          </button>
        )}
      </div>

      {/* Error */}
      {gitCredentialsError && <div className={uiStyles.error}>{gitCredentialsError}</div>}

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {gitCredentialsLoading && gitCredentials.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading repository access tokens...
          </div>
        ) : gitCredentials.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No repository access tokens yet
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Add a GitHub or GitLab token before assigning work that needs private repository
              access.
            </p>
          </div>
        ) : (
          <>
            {gitCredentials.length > 0 && (
              <table className={uiStyles.table} aria-label="Repository access tokens">
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
                  {gitCredentials.map((cred: GitCredential) => (
                    <CredentialRow key={cred.id} credential={cred} onDelete={handleDelete} />
                  ))}
                </tbody>
              </table>
            )}
          </>
        )}

        {/* Add form */}
        {showForm && (
          <AddCredentialForm
            existingProviders={existingProviders}
            onSave={handleSave}
            onCancel={() => setShowForm(false)}
            saving={saving}
          />
        )}
      </div>
    </div>
  )
}
