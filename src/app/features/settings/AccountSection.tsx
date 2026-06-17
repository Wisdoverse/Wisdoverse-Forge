import { useState, useEffect, type FormEvent } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAuth } from '@app/shared/model/auth.context'
import { useTheme } from '@app/shared/model/theme.context'
import { useI18n } from '@app/shared/model/i18n.context'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { getUserApi } from '@app/shared/api/legacy'
import { useNavigationStore } from '@app/entities/navigation'
import { userRoleLabel } from '@app/entities/user'
import { accountErrorMessage } from './accountErrorMessages'

function reportedAccountValue(value: string | null | undefined, fallback: string): string {
  const trimmed = value?.trim()
  return trimmed ? trimmed : fallback
}

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

  async function handleSubmit(e: FormEvent) {
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
      setError(accountErrorMessage('changePassword', err))
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
        <div
          role="status"
          aria-live="polite"
          className="rounded-card border border-apple-blue/20 bg-apple-blue/10 px-3 py-2 text-ui-body text-apple-blue"
        >
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
// Team Space Rename Form
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
        Select a team space from the sidebar before changing team space settings.
      </div>
    )
  }

  const canEdit = currentOrg.role === 'owner' || currentOrg.role === 'admin'
  const saving = pendingOrgIds.has(currentOrg.id)
  const trimmed = name.trim()
  const dirty = trimmed !== currentOrg.name
  const valid = trimmed.length >= 1 && trimmed.length <= 100

  async function handleSubmit(e: FormEvent) {
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
        setError(accountErrorMessage('renameOrganization', err))
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
      {error && (
        <div role="alert" className={uiStyles.error}>
          {error}
        </div>
      )}
      {success && (
        <div
          role="status"
          aria-live="polite"
          className="rounded-card border border-apple-blue/20 bg-apple-blue/10 px-3 py-2 text-ui-body text-apple-blue"
        >
          Team space name updated. Teammates will see the new name in navigation.
        </div>
      )}
      <div>
        <label htmlFor="account-organization-name" className={uiStyles.label}>
          Team Space Name
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
            Only owners and admins can rename this team space.
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
            {saving ? 'Saving...' : 'Save Team Space Name'}
          </button>
        </div>
      )}
    </form>
  )
}

// ============================================================================
// Getting Started Guide Row
// ============================================================================

function GettingStartedGuideRow() {
  const navigate = useNavigate()
  const preferences = useSettingsStore((s) => s.preferences)
  const preferencesLoaded = useSettingsStore((s) => s.preferencesLoaded)
  const loadPreferences = useSettingsStore((s) => s.loadPreferences)
  const setGettingStartedDismissed = useSettingsStore((s) => s.setGettingStartedDismissed)
  const [restoring, setRestoring] = useState(false)
  const [restored, setRestored] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Direct visits to /settings/account need the stored preference; the store
  // skips the request if another surface already loaded it.
  useEffect(() => {
    void loadPreferences()
  }, [loadPreferences])

  const dismissed = preferences?.gettingStartedDismissed === true
  const canOpenChecklist = preferencesLoaded && !dismissed
  const statusLine = !preferencesLoaded
    ? 'Checking whether the setup checklist is hidden...'
    : dismissed
      ? 'The setup checklist is hidden right now.'
      : 'The setup checklist is already visible in the sidebar, so there is nothing to restore.'

  async function handleRestore() {
    setError(null)
    setRestored(false)
    setRestoring(true)
    const ok = await setGettingStartedDismissed(false)
    setRestoring(false)
    if (ok) {
      setRestored(true)
    } else {
      setError(
        'Check your connection, then choose Show setup checklist again. The setup checklist could not be shown.'
      )
    }
  }

  function openChecklist() {
    void navigate({ to: '/start' })
  }

  return (
    <div className="space-y-2 px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="text-ui-body text-foreground-light dark:text-foreground-dark">
            Setup checklist
          </p>
          <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Skipping Start only hides the sidebar shortcut. It does not change projects, agents, or
            tasks. Show it again here when you want the checklist back. {statusLine}
          </p>
        </div>
        <div className="flex shrink-0 flex-col gap-2 sm:flex-row">
          {canOpenChecklist && !restored && (
            <button
              type="button"
              onClick={openChecklist}
              className={cn(
                uiStyles.secondaryButton,
                'inline-flex h-9 items-center justify-center text-apple-blue'
              )}
            >
              Open setup checklist
            </button>
          )}
          <button
            type="button"
            onClick={handleRestore}
            disabled={restoring || !preferencesLoaded || !dismissed}
            className={uiStyles.secondaryButton}
          >
            {restoring ? 'Showing...' : 'Show in sidebar again'}
          </button>
        </div>
      </div>
      {error && (
        <div role="alert" className={cn(uiStyles.error, 'mb-0')}>
          {error}
        </div>
      )}
      {restored && (
        <div
          role="status"
          aria-live="polite"
          className="flex flex-col gap-2 rounded-card border border-apple-blue/20 bg-apple-blue/10 px-3 py-2 text-ui-body text-apple-blue sm:flex-row sm:items-center sm:justify-between"
        >
          <span>
            The setup checklist is back in the sidebar. Open it whenever you want to check setup
            again. Your projects, agents, and tasks were not changed.
          </span>
          <button
            type="button"
            onClick={openChecklist}
            className={cn(
              uiStyles.secondaryButton,
              'inline-flex h-9 shrink-0 items-center justify-center text-apple-blue'
            )}
          >
            Open setup checklist
          </button>
        </div>
      )}
    </div>
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
              {reportedAccountValue(user?.username, 'Refresh this page to load username')}
            </span>
          </div>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Email
            </span>
            <span className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              {reportedAccountValue(user?.email, 'Refresh this page to load email')}
            </span>
          </div>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Access level
            </span>
            <span className={uiStyles.activeBadge}>{userRoleLabel(user?.role)}</span>
          </div>
        </div>
      </div>

      {/* Team Space */}
      <div>
        <h3 className={uiStyles.groupLabel}>Team space</h3>
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

      {/* Setup checklist */}
      <div>
        <h3 className={uiStyles.groupLabel}>Setup checklist</h3>
        <div className={uiStyles.card}>
          <GettingStartedGuideRow />
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
