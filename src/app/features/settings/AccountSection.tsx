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

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError(null)
    setSuccess(false)

    if (form.newPassword !== form.confirmPassword) {
      setError('New passwords do not match')
      return
    }
    if (form.newPassword.length < 8) {
      setError('New password must be at least 8 characters')
      return
    }

    setSaving(true)
    try {
      await getUserApi().changePassword(form.currentPassword, form.newPassword)
      setSuccess(true)
      setForm(DEFAULT_PW_FORM)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to change password')
    } finally {
      setSaving(false)
    }
  }

  const inputClass = cn(uiStyles.input)

  return (
    <form onSubmit={handleSubmit} className="space-y-3">
      {error && <div className={uiStyles.error}>{error}</div>}
      {success && (
        <div className="rounded-card border border-apple-blue/20 bg-apple-blue/10 px-3 py-2 text-ui-body text-apple-blue">
          Password changed successfully
        </div>
      )}
      <div className="grid grid-cols-1 gap-3">
        <div>
          <label className={uiStyles.label}>Current Password</label>
          <input
            type="password"
            value={form.currentPassword}
            onChange={(e) => setForm({ ...form, currentPassword: e.target.value })}
            required
            autoComplete="current-password"
            className={inputClass}
          />
        </div>
        <div>
          <label className={uiStyles.label}>New Password</label>
          <input
            type="password"
            value={form.newPassword}
            onChange={(e) => setForm({ ...form, newPassword: e.target.value })}
            required
            autoComplete="new-password"
            className={inputClass}
          />
        </div>
        <div>
          <label className={uiStyles.label}>Confirm New Password</label>
          <input
            type="password"
            value={form.confirmPassword}
            onChange={(e) => setForm({ ...form, confirmPassword: e.target.value })}
            required
            autoComplete="new-password"
            className={inputClass}
          />
        </div>
      </div>
      <div className="flex justify-end">
        <button
          type="submit"
          disabled={
            saving ||
            !form.currentPassword.trim() ||
            !form.newPassword.trim() ||
            !form.confirmPassword.trim()
          }
          className={uiStyles.primaryButton}
        >
          {saving ? 'Saving...' : 'Change Password'}
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

  if (!currentOrg) return null

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
          Organization renamed successfully
        </div>
      )}
      <div>
        <label className={uiStyles.label}>Organization Name</label>
        <input
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
            {saving ? 'Saving...' : 'Save'}
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
