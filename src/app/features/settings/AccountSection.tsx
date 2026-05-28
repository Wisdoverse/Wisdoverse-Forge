import { useState, useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAuth } from '@app/shared/model/auth.context'
import { useTheme } from '@app/shared/model/theme.context'
import { useI18n } from '@app/shared/model/i18n.context'
import { getUserApi } from '@app/shared/api/legacy'
import { useNavigationStore } from '@app/entities/navigation'

// ============================================================================
// Password Change Form
// ============================================================================

interface PasswordFormState {
  currentPassword: string
  newPassword: string
  confirmPassword: string
}

const DEFAULT_PW_FORM: PasswordFormState = {
  currentPassword: '',
  newPassword: '',
  confirmPassword: '',
}

function PasswordChangeForm() {
  const [form, setForm] = useState<PasswordFormState>(DEFAULT_PW_FORM)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)

  const hasCurrentPassword = form.currentPassword.trim().length > 0
  const hasConfirmation = form.confirmPassword.length > 0
  const newPasswordIsLongEnough = form.newPassword.length >= 8
  const passwordsMatch = hasConfirmation && form.newPassword === form.confirmPassword
  const passwordChecks = [
    {
      id: 'current-password',
      met: hasCurrentPassword,
      label: 'Enter your current password.',
    },
    {
      id: 'new-password-length',
      met: newPasswordIsLongEnough,
      label: 'Use at least 8 characters for the new password.',
    },
    {
      id: 'confirm-password',
      met: passwordsMatch,
      label: hasConfirmation
        ? 'Make the confirmation match the new password.'
        : 'Confirm the new password.',
    },
  ]
  const firstMissingStep = passwordChecks.find((check) => !check.met)
  const canSubmitPasswordChange = !saving && !firstMissingStep
  const passwordStatus = saving
    ? 'Saving your new password...'
    : firstMissingStep
      ? `Next: ${firstMissingStep.label}`
      : 'Ready to update your password.'

  function updateField(field: keyof PasswordFormState, value: string) {
    setForm((current) => ({ ...current, [field]: value }))
    setError(null)
    setSuccess(false)
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError(null)
    setSuccess(false)

    if (form.newPassword !== form.confirmPassword) {
      setError('The two new passwords do not match. Re-enter them and try again.')
      return
    }
    if (form.newPassword.length < 8) {
      setError('Use at least 8 characters for the new password.')
      return
    }

    setSaving(true)
    try {
      await getUserApi().changePassword(form.currentPassword, form.newPassword)
      setSuccess(true)
      setForm(DEFAULT_PW_FORM)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Password was not changed. Try again.')
    } finally {
      setSaving(false)
    }
  }

  const inputClass = cn(uiStyles.input)

  return (
    <form onSubmit={handleSubmit} className="space-y-3">
      <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        Use this only when you know the current password. After saving, sign in with the new
        password next time.
      </p>
      {error && (
        <div role="alert" className={uiStyles.error}>
          {error}
        </div>
      )}
      {success && (
        <div className="rounded-card border border-apple-blue/20 bg-apple-blue/10 px-3 py-2 text-ui-body text-apple-blue">
          Password changed. Use the new password the next time you sign in.
        </div>
      )}
      <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        Enter your current password, then choose a new password with at least 8 characters.
      </p>
      <div className="grid grid-cols-1 gap-3">
        <div>
          <label htmlFor="account-current-password" className={uiStyles.label}>
            Current Password
          </label>
          <input
            id="account-current-password"
            type="password"
            value={form.currentPassword}
            onChange={(e) => updateField('currentPassword', e.target.value)}
            required
            autoComplete="current-password"
            className={inputClass}
            aria-describedby="password-change-status"
          />
        </div>
        <div>
          <label htmlFor="account-new-password" className={uiStyles.label}>
            New Password
          </label>
          <input
            id="account-new-password"
            type="password"
            value={form.newPassword}
            onChange={(e) => updateField('newPassword', e.target.value)}
            required
            autoComplete="new-password"
            className={inputClass}
            aria-describedby="password-change-status password-change-checks"
          />
        </div>
        <div>
          <label htmlFor="account-confirm-password" className={uiStyles.label}>
            Confirm New Password
          </label>
          <input
            id="account-confirm-password"
            type="password"
            value={form.confirmPassword}
            onChange={(e) => updateField('confirmPassword', e.target.value)}
            required
            autoComplete="new-password"
            className={inputClass}
            aria-describedby="password-change-status password-change-checks"
          />
        </div>
      </div>
      <div
        id="password-change-checks"
        className="grid gap-1 rounded-card border border-black/[0.06] bg-black/[0.02] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.03]"
      >
        {passwordChecks.map((check) => (
          <p
            key={check.id}
            className={cn(
              'text-ui-caption',
              check.met ? 'text-apple-blue' : 'text-secondary-light dark:text-secondary-dark'
            )}
          >
            {check.met ? 'Done: ' : 'Needed: '}
            {check.label}
          </p>
        ))}
      </div>
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <p
          id="password-change-status"
          aria-live="polite"
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          {passwordStatus}
        </p>
        <button
          type="submit"
          disabled={!canSubmitPasswordChange}
          className={uiStyles.primaryButton}
        >
          {saving ? 'Saving...' : 'Update Password'}
        </button>
      </div>
    </form>
  )
}

// ============================================================================
// Organization Rename Form
// ============================================================================

function OrgRenameForm() {
  const orgs = useNavigationStore((s) => s.orgs)
  const selectedOrgId = useNavigationStore((s) => s.selectedOrgId)
  const updateOrg = useNavigationStore((s) => s.updateOrg)
  const currentOrg = orgs.find((o) => o.id === selectedOrgId) ?? null

  const [name, setName] = useState(currentOrg?.name ?? '')
  // Track in-flight saves per org id so that switching orgs does not clobber
  // a pending request's disabled state, and a user cannot double-submit the
  // same org by switching away and back.
  const [pendingOrgIds, setPendingOrgIds] = useState<ReadonlySet<string>>(() => new Set())
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)

  // Reset per-org UI state (input, banners) when the selected org changes.
  // `pendingOrgIds` is intentionally NOT reset — pending saves must remain
  // tracked even while the user visits other orgs.
  const orgId = currentOrg?.id ?? null
  useEffect(() => {
    setName(currentOrg?.name ?? '')
    setError(null)
    setSuccess(false)
    // Only re-run on org switch, not on every name update from the store
  }, [orgId])

  if (!currentOrg) {
    return (
      <div className="rounded-card border border-black/[0.08] bg-black/[0.02] px-3 py-2 text-ui-body text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark">
        Select an organization from the sidebar before changing organization settings.
      </div>
    )
  }

  const canEdit = currentOrg.role === 'owner' || currentOrg.role === 'admin'
  const saving = pendingOrgIds.has(currentOrg.id)
  const trimmed = name.trim()
  const dirty = trimmed !== currentOrg.name
  const valid = trimmed.length >= 1 && trimmed.length <= 100

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!currentOrg || !dirty || !valid || saving) return
    // Capture the org id at submit time. If the user switches orgs mid-save,
    // the success/error message must not leak onto the newly selected org.
    const submittingOrgId = currentOrg.id
    setError(null)
    setSuccess(false)
    setPendingOrgIds((prev) => {
      const next = new Set(prev)
      next.add(submittingOrgId)
      return next
    })
    try {
      await updateOrg(submittingOrgId, { name: trimmed })
      if (useNavigationStore.getState().selectedOrgId === submittingOrgId) {
        // Sync the input to the trimmed value we persisted, so a leading/
        // trailing space doesn't leave a "clean" form showing a value that
        // no longer matches the saved name.
        setName(trimmed)
        setSuccess(true)
      }
    } catch (err) {
      if (useNavigationStore.getState().selectedOrgId === submittingOrgId) {
        setError(err instanceof Error ? err.message : 'Failed to rename organization')
      }
    } finally {
      setPendingOrgIds((prev) => {
        const next = new Set(prev)
        next.delete(submittingOrgId)
        return next
      })
    }
  }

  const inputClass = cn(uiStyles.input, 'disabled:opacity-60 disabled:cursor-not-allowed')

  return (
    <form onSubmit={handleSubmit} className="space-y-3">
      {error && <div className={uiStyles.error}>{error}</div>}
      {success && (
        <div className="rounded-card border border-apple-blue/20 bg-apple-blue/10 px-3 py-2 text-ui-body text-apple-blue">
          Organization name updated. Teammates will see the new name in navigation.
        </div>
      )}
      <div>
        <label htmlFor="account-organization-name" className={uiStyles.label}>
          Organization Name
        </label>
        <input
          id="account-organization-name"
          type="text"
          value={name}
          onChange={(e) => {
            setName(e.target.value)
            setSuccess(false)
          }}
          maxLength={100}
          disabled={!canEdit || saving}
          className={inputClass}
        />
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          This changes the display name only. Projects, teams, and permissions stay where they are.
        </p>
        {!canEdit && (
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Only owners and admins can rename this organization.
          </p>
        )}
      </div>
      {canEdit && (
        <div className="flex justify-end">
          <button
            type="submit"
            disabled={saving || !dirty || !valid}
            className={uiStyles.primaryButton}
          >
            {saving ? 'Saving...' : 'Save Organization Name'}
          </button>
        </div>
      )}
    </form>
  )
}

// ============================================================================
// AccountSection
// ============================================================================

export function AccountSection() {
  const { user } = useAuth()
  const { theme, toggleTheme } = useTheme()
  const { language, setLanguage } = useI18n()

  return (
    <div className="space-y-6">
      {/* Section header */}
      <div>
        <h2 className={uiStyles.sectionTitle}>Account</h2>
        <p className={uiStyles.sectionDescription}>Manage your account settings and appearance</p>
      </div>

      {/* Profile info */}
      <div>
        <h3 className={uiStyles.groupLabel}>Profile</h3>
        <div className={cn(uiStyles.card, 'divide-y divide-[rgb(var(--border))]')}>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Username
            </span>
            <span className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              {user?.username ?? '—'}
            </span>
          </div>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Email
            </span>
            <span className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              {user?.email ?? '—'}
            </span>
          </div>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-secondary-light dark:text-secondary-dark">Role</span>
            <span className={cn(uiStyles.activeBadge, 'capitalize')}>{user?.role ?? 'user'}</span>
          </div>
        </div>
      </div>

      {/* Organization */}
      <div>
        <h3 className={uiStyles.groupLabel}>Organization</h3>
        <div className={uiStyles.cardPadded}>
          <OrgRenameForm />
        </div>
      </div>

      {/* Appearance */}
      <div>
        <h3 className={uiStyles.groupLabel}>Appearance</h3>
        <div className={cn(uiStyles.card, 'divide-y divide-[rgb(var(--border))]')}>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-foreground-light dark:text-foreground-dark">
              Theme
            </span>
            <button type="button" onClick={toggleTheme} className={uiStyles.secondaryButton}>
              {theme === 'light' ? 'Switch to Dark' : 'Switch to Light'}
            </button>
          </div>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-foreground-light dark:text-foreground-dark">
              Language
            </span>
            <select
              value={language}
              onChange={(e) => setLanguage(e.target.value as 'en' | 'zh')}
              className={uiStyles.select}
            >
              <option value="en">English</option>
              <option value="zh">中文</option>
            </select>
          </div>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-foreground-light dark:text-foreground-dark">
              Version
            </span>
            <span className="text-ui-body text-secondary-light dark:text-secondary-dark">
              0.1.0
            </span>
          </div>
        </div>
      </div>

      {/* Password change */}
      <div>
        <h3 className={uiStyles.groupLabel}>Security</h3>
        <div className={uiStyles.cardPadded}>
          <p className="mb-3 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            Change Password
          </p>
          <PasswordChangeForm />
        </div>
      </div>
    </div>
  )
}
