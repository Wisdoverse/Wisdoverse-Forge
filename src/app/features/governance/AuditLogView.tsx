import { useCallback, useEffect, useMemo, useState, type FormEvent, type ReactNode } from 'react'
import {
  ClipboardCheck,
  Download,
  EyeOff,
  Fingerprint,
  RefreshCw,
  Search,
  ShieldAlert,
  ShieldCheck,
  ShieldQuestion,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import {
  orchestrationApi,
  type GovernanceAuditEntry,
  type GovernanceAuditItemKind,
  type GovernanceAuditQueryParams,
  type GovernanceAuditResponse,
  type GovernanceAuditScopeKind,
  type GovernanceAuditTamperStatus,
} from '@app/shared/api/orchestration'

type ItemKindFilter = 'all' | GovernanceAuditItemKind
type ScopeKindFilter = 'all' | GovernanceAuditScopeKind

interface FilterState {
  eventPrefix: string
  eventType: string
  itemKind: ItemKindFilter
  scopeKind: ScopeKindFilter
  scopeId: string
  userId: string
  from: string
  to: string
  redactSecrets: boolean
  limit: number
}

interface QuickAuditView {
  id: string
  label: string
  description: string
  Icon: LucideIcon
  filters: Partial<FilterState>
}

const DEFAULT_FILTERS: FilterState = {
  eventPrefix: 'governance.context.',
  eventType: '',
  itemKind: 'all',
  scopeKind: 'all',
  scopeId: '',
  userId: '',
  from: '',
  to: '',
  redactSecrets: true,
  limit: 50,
}

const QUICK_AUDIT_VIEWS: QuickAuditView[] = [
  {
    id: 'all',
    label: 'All governance events',
    description: 'Start broad, then narrow the result.',
    Icon: Search,
    filters: {},
  },
  {
    id: 'skill-decisions',
    label: 'Skill decisions',
    description: 'Review approvals and changes for skills.',
    Icon: ClipboardCheck,
    filters: {
      eventPrefix: 'governance.context.skill.',
      itemKind: 'skill',
    },
  },
  {
    id: 'memory-feedback',
    label: 'Memory feedback',
    description: 'See saved context feedback first.',
    Icon: ShieldCheck,
    filters: {
      eventType: 'governance.context.feedback.recorded',
      itemKind: 'memory',
    },
  },
]

const ITEM_KIND_OPTIONS: { value: ItemKindFilter; label: string }[] = [
  { value: 'all', label: 'All items' },
  { value: 'memory', label: 'Memory' },
  { value: 'skill', label: 'Skill' },
]

const SCOPE_KIND_OPTIONS: { value: ScopeKindFilter; label: string }[] = [
  { value: 'all', label: 'All areas' },
  { value: 'org', label: 'Organization' },
  { value: 'workspace', label: 'Workspace' },
  { value: 'team', label: 'Team' },
  { value: 'project', label: 'Project' },
  { value: 'user', label: 'User' },
]

const COMMON_EVENT_TYPES = [
  'governance.context.feedback.recorded',
  'governance.context.skill.approved',
  'governance.context.skill.reviewed',
  'governance.context.memory.updated',
  'governance.context.memory.rejected',
]

const INPUT_CLASS =
  'h-9 w-full rounded-full border border-black/[0.08] bg-white px-3 text-ui-caption text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70'

export function AuditLogView() {
  const [filters, setFilters] = useState<FilterState>(DEFAULT_FILTERS)
  const [data, setData] = useState<GovernanceAuditResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [exporting, setExporting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [exportStatus, setExportStatus] = useState<string | null>(null)

  const entries = data?.entries ?? []
  const hiddenRawIds = useMemo(() => entries.filter((entry) => !entry.rawItemId).length, [entries])
  const redactedRows = useMemo(
    () => entries.filter((entry) => entry.detailsRedacted).length,
    [entries]
  )
  const activeQuickViewId = useMemo(
    () => QUICK_AUDIT_VIEWS.find((view) => quickViewMatches(filters, view))?.id ?? null,
    [filters]
  )

  const loadAudit = useCallback(async (nextFilters: FilterState) => {
    setLoading(true)
    setError(null)
    setExportStatus(null)
    try {
      const response = await orchestrationApi.fetchGovernanceAudit(buildQuery(nextFilters))
      setData(response)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load governance audit')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadAudit(DEFAULT_FILTERS)
  }, [loadAudit])

  function updateFilter<K extends keyof FilterState>(key: K, value: FilterState[K]) {
    setFilters((current) => ({ ...current, [key]: value }))
  }

  function submitFilters(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    void loadAudit(filters)
  }

  function applyQuickView(view: QuickAuditView) {
    const nextFilters = { ...DEFAULT_FILTERS, ...view.filters }
    setFilters(nextFilters)
    void loadAudit(nextFilters)
  }

  async function exportAudit() {
    setExporting(true)
    setError(null)
    setExportStatus(null)
    try {
      const response = await orchestrationApi.exportGovernanceAudit(buildQuery(filters))
      const body = JSON.stringify(response, null, 2)
      const blob = new Blob([body], { type: 'application/json' })
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = `context-governance-audit-${new Date().toISOString()}.json`
      link.click()
      URL.revokeObjectURL(url)
      setData(response)
      setExportStatus(
        `${response.entries.length} audit ${response.entries.length === 1 ? 'event' : 'events'} exported`
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to export governance audit')
    } finally {
      setExporting(false)
    }
  }

  return (
    <div data-testid="governance-audit-view" className="flex h-full flex-col">
      <form
        onSubmit={submitFilters}
        className="shrink-0 border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.06] sm:px-6"
      >
        <div className="mb-4 flex flex-col gap-2">
          <div>
            <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
              Start with what you need to check
            </p>
            <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Pick a common audit view, then narrow it by item, work area, person, or time.
            </p>
          </div>
          <div role="group" aria-label="Common audit views" className="grid gap-2 sm:grid-cols-3">
            {QUICK_AUDIT_VIEWS.map((view) => (
              <QuickAuditButton
                key={view.id}
                view={view}
                active={activeQuickViewId === view.id}
                disabled={loading}
                onClick={() => applyQuickView(view)}
              />
            ))}
          </div>
        </div>
        <div className="grid gap-3 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1.2fr)_160px_160px_160px_auto]">
          <Field label="Event family">
            <input
              data-testid="governance-audit-filter-event-prefix"
              name="eventPrefix"
              autoComplete="off"
              value={filters.eventPrefix}
              onChange={(event) => updateFilter('eventPrefix', event.target.value)}
              placeholder="governance.context."
              className={INPUT_CLASS}
            />
          </Field>
          <Field label="Exact event name">
            <input
              data-testid="governance-audit-filter-event-type"
              name="eventType"
              autoComplete="off"
              value={filters.eventType}
              onChange={(event) => updateFilter('eventType', event.target.value)}
              placeholder="Pick a view or paste an event name"
              className={INPUT_CLASS}
            />
            <datalist id="governance-audit-event-type-options">
              {COMMON_EVENT_TYPES.map((eventType) => (
                <option key={eventType} value={eventType} />
              ))}
            </datalist>
          </Field>
          <Field label="Changed item">
            <select
              data-testid="governance-audit-filter-item-kind"
              name="itemKind"
              value={filters.itemKind}
              onChange={(event) => updateFilter('itemKind', event.target.value as ItemKindFilter)}
              className={INPUT_CLASS}
            >
              {ITEM_KIND_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Area">
            <select
              data-testid="governance-audit-filter-scope-kind"
              name="scopeKind"
              value={filters.scopeKind}
              onChange={(event) => updateFilter('scopeKind', event.target.value as ScopeKindFilter)}
              className={INPUT_CLASS}
            >
              {SCOPE_KIND_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Record limit">
            <input
              type="number"
              name="limit"
              min={1}
              max={200}
              inputMode="numeric"
              autoComplete="off"
              value={filters.limit}
              onChange={(event) => updateFilter('limit', Number(event.target.value))}
              className={INPUT_CLASS}
            />
          </Field>
          <div className="flex items-end gap-2">
            <button
              type="submit"
              disabled={loading}
              className="inline-flex h-9 items-center gap-2 rounded-full bg-apple-blue px-3 text-ui-button font-semibold text-white transition-colors hover:bg-apple-blue-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60"
            >
              <Search size={15} aria-hidden="true" />
              Apply filters
            </button>
            <button
              type="button"
              data-testid="governance-audit-refresh"
              onClick={() => void loadAudit(filters)}
              disabled={loading}
              aria-label="Refresh audit events"
              className="inline-flex h-9 w-9 items-center justify-center rounded-full border border-black/[0.08] bg-white text-ui-button text-foreground-light transition-colors hover:bg-black/[0.03] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60 dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:text-foreground-dark dark:hover:bg-white/[0.06]"
              title="Refresh audit events"
            >
              <RefreshCw size={15} className={cn(loading && 'animate-spin')} aria-hidden="true" />
            </button>
            <button
              type="button"
              data-testid="governance-audit-export"
              onClick={() => void exportAudit()}
              disabled={exporting}
              aria-label="Export audit events"
              className="inline-flex h-9 w-9 items-center justify-center rounded-full border border-black/[0.08] bg-white text-ui-button text-foreground-light transition-colors hover:bg-black/[0.03] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60 dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:text-foreground-dark dark:hover:bg-white/[0.06]"
              title="Export audit events"
            >
              <Download size={15} aria-hidden="true" />
            </button>
          </div>
        </div>

        <div className="mt-3 grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_180px_180px_auto]">
          <Field label="Work area ID">
            <input
              value={filters.scopeId}
              name="scopeId"
              autoComplete="off"
              onChange={(event) => updateFilter('scopeId', event.target.value)}
              placeholder="Paste an org, workspace, team, or project ID"
              className={INPUT_CLASS}
            />
          </Field>
          <Field label="Person ID">
            <input
              value={filters.userId}
              name="userId"
              autoComplete="off"
              onChange={(event) => updateFilter('userId', event.target.value)}
              placeholder="Paste a user ID when needed"
              className={INPUT_CLASS}
            />
          </Field>
          <Field label="From" help="Optional start time.">
            <input
              type="datetime-local"
              name="from"
              autoComplete="off"
              value={filters.from}
              onChange={(event) => updateFilter('from', event.target.value)}
              className={INPUT_CLASS}
            />
          </Field>
          <Field label="To" help="Optional end time.">
            <input
              type="datetime-local"
              name="to"
              autoComplete="off"
              value={filters.to}
              onChange={(event) => updateFilter('to', event.target.value)}
              className={INPUT_CLASS}
            />
          </Field>
          <label className="flex h-full min-h-14 items-end gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
            <input
              type="checkbox"
              name="redactSecrets"
              checked={filters.redactSecrets}
              onChange={(event) => updateFilter('redactSecrets', event.target.checked)}
              className="mb-2 h-4 w-4 rounded border-black/20 text-apple-blue focus:ring-apple-blue"
            />
            <span className="pb-2">Hide secrets before export</span>
          </label>
        </div>
      </form>

      <div className="flex-1 overflow-auto p-4 sm:p-6">
        {error && (
          <div
            role="alert"
            className="mb-4 rounded-card border border-apple-red/20 bg-apple-red/10 px-4 py-2 text-ui-body text-apple-red"
          >
            {error}
          </div>
        )}
        {exportStatus && (
          <div
            aria-live="polite"
            className="mb-4 rounded-card border border-apple-blue/20 bg-apple-blue/10 px-4 py-2 text-ui-body text-apple-blue"
          >
            {exportStatus}
          </div>
        )}

        <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
          <Metric label="Events" value={entries.length} />
          <Metric label="View" value={data?.query.eventPrefix ?? filters.eventPrefix} compact />
          <Metric label="Protected subjects" value={hiddenRawIds} />
          <Metric label="Safe details" value={redactedRows} />
        </div>

        <div className="overflow-hidden rounded-card border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2c2c2e]">
          <div className="overflow-x-auto">
            <table className="w-full min-w-[1120px] text-left text-ui-caption">
              <thead className="border-b border-black/[0.06] bg-black/[0.025] text-ui-caption text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.03] dark:text-secondary-dark">
                <tr>
                  <th className="px-4 py-3 font-semibold">Time</th>
                  <th className="px-4 py-3 font-semibold">Change</th>
                  <th className="px-4 py-3 font-semibold">Changed item</th>
                  <th className="px-4 py-3 font-semibold">Area</th>
                  <th className="px-4 py-3 font-semibold">Changed by</th>
                  <th className="px-4 py-3 font-semibold">Verification</th>
                  <th className="px-4 py-3 font-semibold">Change details</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-black/5 dark:divide-white/10">
                {loading ? (
                  <tr>
                    <td colSpan={7} className="px-4 py-12 text-center text-secondary-light">
                      Loading audit records…
                    </td>
                  </tr>
                ) : entries.length === 0 ? (
                  <tr>
                    <td colSpan={7} className="px-4 py-12 text-center">
                      <p className="font-semibold text-foreground-light dark:text-foreground-dark">
                        No audit events in this view
                      </p>
                      <p className="mt-1 text-secondary-light dark:text-secondary-dark">
                        Try All governance events or widen the time range.
                      </p>
                    </td>
                  </tr>
                ) : (
                  entries.map((entry) => <AuditRow key={entry.id} entry={entry} />)
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  )
}

function QuickAuditButton({
  view,
  active,
  disabled,
  onClick,
}: {
  view: QuickAuditView
  active: boolean
  disabled: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        'flex min-h-16 items-center gap-3 rounded-lg border px-3 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60',
        active
          ? 'border-apple-blue/45 bg-apple-blue/[0.08]'
          : 'border-black/[0.08] bg-white hover:border-apple-blue/35 dark:border-white/[0.1] dark:bg-white/[0.04]'
      )}
    >
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-black/[0.04] text-apple-blue dark:bg-white/[0.06]">
        <view.Icon size={15} strokeWidth={2.25} aria-hidden="true" />
      </span>
      <span className="min-w-0">
        <span className="block truncate text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
          {view.label}
        </span>
        <span className="mt-0.5 block text-ui-caption text-secondary-light dark:text-secondary-dark">
          {view.description}
        </span>
      </span>
    </button>
  )
}

function AuditRow({ entry }: { entry: GovernanceAuditEntry }) {
  return (
    <tr
      data-testid="governance-audit-row"
      className="align-top hover:bg-black/[0.025] dark:hover:bg-white/[0.03]"
    >
      <td className="w-40 px-4 py-3 text-secondary-light dark:text-secondary-dark">
        <span className="tabular-nums">{formatDate(entry.createdAt)}</span>
      </td>
      <td className="max-w-[260px] px-4 py-3">
        <div className="truncate font-mono text-ui-caption" title={entry.eventType}>
          {entry.eventType}
        </div>
        <div className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {entry.itemKind ?? 'hidden item'} · {entry.resourceType}
        </div>
      </td>
      <td className="w-72 px-4 py-3">
        {entry.rawItemId ? (
          <SubjectLine
            testId="governance-audit-raw-item-id"
            icon="visible"
            value={entry.rawItemId}
          />
        ) : (
          <SubjectLine
            testId="governance-audit-subject-hash"
            icon="hash"
            value={entry.auditSubjectHash}
          />
        )}
        {entry.detailsRedacted && (
          <div
            data-testid="governance-audit-redacted"
            className="mt-2 inline-flex items-center gap-1 rounded-full bg-black/[0.04] px-2 py-0.5 text-ui-caption font-medium text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark"
          >
            <EyeOff size={12} aria-hidden="true" />
            Protected
          </div>
        )}
      </td>
      <td className="w-56 px-4 py-3">
        <div className="font-medium">{entry.scopeKind ?? 'hidden area'}</div>
        <div className="mt-1 truncate font-mono text-ui-caption text-secondary-light dark:text-secondary-dark">
          {entry.scopeId ?? 'not shared'}
        </div>
      </td>
      <td className="w-48 px-4 py-3">
        <span className="block truncate font-mono text-ui-caption text-secondary-light dark:text-secondary-dark">
          {entry.actorUserId ?? 'System'}
        </span>
      </td>
      <td className="w-44 px-4 py-3">
        <TamperBadge status={entry.tamperStatus} />
      </td>
      <td className="min-w-[260px] px-4 py-3">
        <pre className="max-h-32 overflow-auto rounded-card bg-black/[0.035] p-2 font-mono text-ui-caption leading-relaxed text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark">
          {prettyDetails(entry.details)}
        </pre>
      </td>
    </tr>
  )
}

function SubjectLine({
  testId,
  icon,
  value,
}: {
  testId: string
  icon: 'visible' | 'hash'
  value: string
}) {
  const Icon = icon === 'visible' ? ShieldCheck : Fingerprint
  return (
    <div data-testid={testId} className="flex min-w-0 items-center gap-2">
      <Icon size={14} className="shrink-0 text-apple-blue" aria-hidden="true" />
      <span className="truncate font-mono text-ui-caption" title={value}>
        {shortId(value)}
      </span>
    </div>
  )
}

function TamperBadge({ status }: { status: GovernanceAuditTamperStatus }) {
  const config = {
    valid: {
      Icon: ShieldCheck,
      className: 'bg-apple-blue/10 text-apple-blue',
      label: 'Verified',
    },
    invalid: {
      Icon: ShieldAlert,
      className: 'bg-apple-red/10 text-apple-red',
      label: 'Needs review',
    },
    not_configured: {
      Icon: ShieldQuestion,
      className:
        'bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
      label: 'Not checked',
    },
  }[status]
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-ui-caption font-medium',
        config.className
      )}
    >
      <config.Icon size={13} aria-hidden="true" />
      {config.label}
    </span>
  )
}

function Metric({
  label,
  value,
  compact = false,
}: {
  label: string
  value: string | number
  compact?: boolean
}) {
  return (
    <div className="rounded-card border border-black/[0.08] bg-white px-4 py-3 dark:border-white/[0.1] dark:bg-[#2c2c2e]">
      <div className="text-ui-caption font-semibold text-secondary-light dark:text-secondary-dark">
        {label}
      </div>
      <div
        className={cn(
          'mt-1 truncate font-semibold text-foreground-light dark:text-foreground-dark',
          compact ? 'font-mono text-ui-body' : 'text-ui-metric tabular-nums'
        )}
        title={String(value)}
      >
        {value}
      </div>
    </div>
  )
}

function Field({
  label,
  help,
  helpId,
  children,
}: {
  label: string
  help?: string
  helpId?: string
  children: ReactNode
}) {
  return (
    <label className="flex min-w-0 flex-col gap-1">
      <span className="text-ui-caption font-semibold text-secondary-light dark:text-secondary-dark">
        {label}
      </span>
      {children}
      {help && (
        <span
          id={helpId}
          className="text-ui-caption leading-snug text-secondary-light dark:text-secondary-dark"
        >
          {help}
        </span>
      )}
    </label>
  )
}

function quickViewMatches(filters: FilterState, view: QuickAuditView): boolean {
  const expected = { ...DEFAULT_FILTERS, ...view.filters }
  return (
    filters.eventPrefix === expected.eventPrefix &&
    filters.eventType === expected.eventType &&
    filters.itemKind === expected.itemKind &&
    filters.scopeKind === expected.scopeKind &&
    filters.scopeId === expected.scopeId &&
    filters.userId === expected.userId &&
    filters.from === expected.from &&
    filters.to === expected.to &&
    filters.redactSecrets === expected.redactSecrets &&
    filters.limit === expected.limit
  )
}

function buildQuery(filters: FilterState): GovernanceAuditQueryParams {
  return {
    eventPrefix: trimOrUndefined(filters.eventPrefix),
    eventType: trimOrUndefined(filters.eventType),
    itemKind: filters.itemKind === 'all' ? undefined : filters.itemKind,
    scopeKind: filters.scopeKind === 'all' ? undefined : filters.scopeKind,
    scopeId: trimOrUndefined(filters.scopeId),
    userId: trimOrUndefined(filters.userId),
    from: toIsoString(filters.from),
    to: toIsoString(filters.to),
    redactSecrets: filters.redactSecrets,
    limit: Number.isFinite(filters.limit) ? filters.limit : DEFAULT_FILTERS.limit,
    offset: 0,
  }
}

function trimOrUndefined(value: string): string | undefined {
  const trimmed = value.trim()
  return trimmed ? trimmed : undefined
}

function toIsoString(value: string): string | undefined {
  if (!value) return undefined
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString()
}

function formatDate(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(date)
}

function shortId(value: string): string {
  return value.length <= 22 ? value : `${value.slice(0, 10)}…${value.slice(-8)}`
}

function prettyDetails(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}
