import { useEffect, useState, type FormEvent } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { GitCredential, GitProvider } from '@app/entities/agent'
import { formatAccessDate } from './formatAccessDate'

const PROVIDER_LABELS: Record<GitProvider, string> = {
  github: 'GitHub',
  gitlab: 'GitLab',
}

const GIT_CREDENTIAL_SETUP_STEPS = [
  { label: 'Choose where your code lives', value: 'Pick GitHub or GitLab.' },
  {
    label: 'Create a code access key',
    value: 'Create an access key on GitHub or GitLab and allow it to read the code agents need.',
  },
  {
    label: 'Use the normal website by default',
    value:
      'Leave the website address empty for github.com or gitlab.com. Add one only for a company-hosted site.',
  },
]

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
      title: 'Next: Create a code access key',
      detail:
        'Create the key in GitHub or GitLab, paste it here, then agents can open the code you allow.',
      error: 'Paste the code access key from GitHub or GitLab before saving.',
      fieldId: tokenInputId,
    }
  }

  return {
    ready: true,
    title: 'Ready to save',
    detail: 'Save code access, then use a small agent task to confirm it works.',
    error: null,
    fieldId: null,
  }
}

// ============================================================================
// Credential Row
// ============================================================================

interface CredentialRowProps {
  credential: GitCredential
  onDelete: (id: string) => Promise<boolean>
}

function CredentialRow({ credential, onDelete }: CredentialRowProps) {
  const [confirming, setConfirming] = useState(false)
  const [removing, setRemoving] = useState(false)
  const removeWarningId = `code-access-remove-warning-${credential.id}`

  async function handleDelete() {
    if (!confirming) {
      setConfirming(true)
      return
    }
    setRemoving(true)
    const removed = await onDelete(credential.id)
    setRemoving(false)
    if (removed) setConfirming(false)
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
          {credential.host ?? defaultCodeHostLabel(credential.provider)}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {formatAccessDate(credential.createdAt, {
            missing: 'Refresh code access to load added date',
            invalid: 'Refresh code access to check added date',
          })}
        </span>
      </td>
      <td className={cn(uiStyles.tableCell, 'text-right')}>
        <div className="flex flex-wrap justify-end gap-2">
          {confirming && (
            <button
              type="button"
              onClick={() => setConfirming(false)}
              disabled={removing}
              className={uiStyles.subtleButton}
            >
              Keep access
            </button>
          )}
          <button
            type="button"
            onClick={() => void handleDelete()}
            disabled={removing}
            aria-label={
              confirming
                ? `Confirm removing ${PROVIDER_LABELS[credential.provider]} code access`
                : `Remove ${PROVIDER_LABELS[credential.provider]} code access`
            }
            aria-describedby={confirming ? removeWarningId : undefined}
            aria-busy={removing || undefined}
            className={confirming ? uiStyles.dangerConfirmButton : uiStyles.dangerButton}
          >
            {removing ? 'Removing...' : confirming ? 'Remove access now' : 'Remove'}
          </button>
        </div>
        {confirming && (
          <p id={removeWarningId} className="ml-auto mt-1 max-w-48 text-ui-caption text-apple-red">
            Removing this access can stop agents from opening private code on{' '}
            {PROVIDER_LABELS[credential.provider]}.
          </p>
        )}
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
  const tokenIntroId = 'git-credential-token-intro'
  const tokenSafetyId = 'git-credential-token-safety'
  const tokenErrorId = 'git-credential-token-error'
  const hostHelpId = 'git-credential-host-help'
  const hostCompanyHelpId = 'git-credential-host-company-help'
  const readiness = credentialFormReadiness({ token: form.token, tokenInputId })
  const visibleError = submitAttempted && !readiness.ready ? readiness.error : null

  async function handleSubmit(e: FormEvent) {
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
      <div className="mb-3 rounded-lg border border-black/[0.06] bg-white px-3 py-2.5 dark:border-white/[0.08] dark:bg-black/20">
        <div className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          Add code access
        </div>
        <div className="mt-2 grid gap-1.5 sm:grid-cols-3">
          {GIT_CREDENTIAL_SETUP_STEPS.map((step) => (
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

      <div
        className="mb-3 text-ui-caption text-secondary-light dark:text-secondary-dark"
        aria-live="polite"
      >
        <span className="font-medium text-foreground-light dark:text-foreground-dark">
          {readiness.title}
        </span>
        <span> {readiness.detail}</span>
      </div>

      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div>
          <label htmlFor="git-credential-provider" className={uiStyles.label}>
            Git service
          </label>
          <select
            id={providerInputId}
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
            Choose the site that owns the repository.
          </p>
        </div>

        <div>
          <label htmlFor="git-credential-token" className={uiStyles.label}>
            Code access key <span className="text-red-500">*</span>
          </label>
          <p
            id={tokenIntroId}
            className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Paste the code access key from GitHub or GitLab. If that page says personal access
            token, use that value here.
          </p>
          <input
            id="git-credential-token"
            type="password"
            name="token"
            value={form.token}
            onChange={(e) => setForm({ ...form, token: e.target.value })}
            placeholder="Paste the code access key from GitHub or GitLab"
            required
            className={uiStyles.input}
            aria-invalid={visibleError !== null}
            aria-describedby={`${tokenIntroId} ${tokenSafetyId}${visibleError ? ` ${tokenErrorId}` : ''}`}
          />
          <p
            id={tokenSafetyId}
            className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            It lets agents open only the code projects you allow. Do not paste your GitHub or GitLab
            password. This key is hidden after saving.
          </p>
          {visibleError && (
            <p id={tokenErrorId} role="alert" className="mt-1 text-ui-caption text-apple-red">
              {visibleError}
            </p>
          )}
        </div>

        <div className="sm:col-span-2">
          <label htmlFor="git-credential-host" className={uiStyles.label}>
            Company GitHub or GitLab website{' '}
            <span className="text-secondary-light dark:text-secondary-dark font-normal">
              (optional)
            </span>
          </label>
          <p
            id={hostHelpId}
            className="mb-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Leave this empty if you use github.com or gitlab.com.
          </p>
          <input
            id="git-credential-host"
            type="text"
            name="host"
            value={form.host}
            onChange={(e) => setForm({ ...form, host: e.target.value })}
            placeholder="e.g. gitlab.example.com"
            className={uiStyles.input}
            aria-describedby={`${hostHelpId} ${hostCompanyHelpId}`}
          />
          <p
            id={hostCompanyHelpId}
            className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            For a company-hosted Git service, enter the address you open in the browser.
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
          {saving ? 'Saving...' : 'Save code access'}
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
  const [savedMessage, setSavedMessage] = useState<string | null>(null)

  useEffect(() => {
    void loadGitCredentials()
  }, [loadGitCredentials])

  async function handleSave(provider: GitProvider, token: string, host?: string) {
    setSaving(true)
    const ok = await saveGitCredential(provider, token, host)
    setSaving(false)
    if (ok) {
      setShowForm(false)
      setSavedMessage(
        'Code access saved. Create a small task with a private repository link to confirm agents can open it. If it cannot read the repository, come back here and replace this key.'
      )
    }
  }

  async function handleDelete(id: string) {
    return deleteGitCredential(id)
  }

  function openCreateForm() {
    setSavedMessage(null)
    setShowForm(true)
  }

  const existingProviders = gitCredentials.map((c) => c.provider)
  const canAddMore = existingProviders.length < 2

  const tableHeaders: { label: string; className?: string }[] = [
    { label: 'Git service' },
    { label: 'Website address' },
    { label: 'Added on' },
    { label: '', className: 'w-20' },
  ]

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>HTTPS code access</h2>
          <p className={uiStyles.sectionDescription}>
            Use this when a private code link starts with https://. If it starts with git@, use SSH
            code access instead.
          </p>
        </div>
        {!showForm && canAddMore && (
          <button type="button" onClick={openCreateForm} className={uiStyles.primaryButton}>
            <span>+</span>
            <span>Add HTTPS code access</span>
          </button>
        )}
      </div>

      {/* Error */}
      {gitCredentialsError && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {gitCredentialsError}
        </div>
      )}

      {savedMessage && (
        <div
          role="status"
          aria-live="polite"
          className="mb-3 rounded-card border border-apple-blue/20 bg-apple-blue/10 px-3 py-2 text-ui-body text-apple-blue"
        >
          {savedMessage}
        </div>
      )}

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {gitCredentialsLoading && gitCredentials.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading code access...
          </div>
        ) : gitCredentials.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center" data-testid="code-access-empty-state">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Let agents open private HTTPS code links
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Use this for GitHub or GitLab links that start with https://, such as
              https://github.com/team/repo.git. If the address starts with git@, use SSH code access
              instead.
            </p>
            {canAddMore && (
              <button
                type="button"
                onClick={openCreateForm}
                className={cn(uiStyles.primaryButton, 'mt-3')}
              >
                <span>+</span>
                <span>Add HTTPS code access</span>
              </button>
            )}
          </div>
        ) : (
          <>
            {gitCredentials.length > 0 && (
              <table className={uiStyles.table} aria-label="Code access">
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

function defaultCodeHostLabel(provider: GitProvider): string {
  return provider === 'github' ? 'github.com' : 'gitlab.com'
}
