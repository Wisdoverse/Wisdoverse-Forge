import { useCallback, useEffect, useMemo, useState, type FormEvent, type ReactNode } from 'react'
import {
  ClipboardCheck,
  Download,
  EyeOff,
  Fingerprint,
  RefreshCw,
  Search,
  ShieldCheck,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import {
  orchestrationApi,
  type GovernanceAuditEntry,
  type GovernanceAuditItemKind,
  type GovernanceAuditQueryParams,
  type GovernanceAuditResponse,
  type GovernanceAuditScopeKind,
  type GovernanceAuditTamperStatus,
} from '@app/shared/api/orchestration'
import { governanceAuditErrorMessage } from './governanceAuditErrorMessages'

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
    label: 'All saved item changes',
    description: 'See every saved note and saved guidance change.',
    Icon: Search,
    filters: {},
  },
  {
    id: 'skill-decisions',
    label: 'Saved guidance decisions',
    description: 'Check who saved or updated saved guidance.',
    Icon: ClipboardCheck,
    filters: {
      eventPrefix: 'governance.context.skill.',
      itemKind: 'skill',
    },
  },
  {
    id: 'memory-feedback',
    label: 'Saved note feedback',
    description: 'See whether saved notes helped or caused trouble.',
    Icon: ShieldCheck,
    filters: {
      eventType: 'governance.context.feedback.recorded',
      itemKind: 'memory',
    },
  },
]

const ITEM_KIND_OPTIONS: { value: ItemKindFilter; label: string }[] = [
  { value: 'all', label: 'All items' },
  { value: 'memory', label: 'Saved note' },
  { value: 'skill', label: 'Saved guidance' },
]

const SCOPE_KIND_OPTIONS: { value: ScopeKindFilter; label: string }[] = [
  { value: 'all', label: 'All areas' },
  { value: 'org', label: 'Team space' },
  { value: 'workspace', label: 'Work area' },
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

const INPUT_CLASS = uiStyles.input
const SELECT_CLASS = cn(uiStyles.select, 'w-full')
const HIDDEN_AUDIT_DETAIL_VALUE =
  'Hidden for safety. Keep secrets hidden, choose Check change history again, then export again.'
const MISSING_AUDIT_ACCESS_MESSAGE =
  'Reconnect the needed account access, then choose Check change history again. This saved change needs access before it can be shown.'
const REPEATED_AUDIT_DETAIL_VALUE = 'Repeated detail omitted.'

export function AuditLogView() {
  const [filters, setFilters] = useState<FilterState>(DEFAULT_FILTERS)
  const [data, setData] = useState<GovernanceAuditResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [exporting, setExporting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [exportStatus, setExportStatus] = useState<string | null>(null)

  const entries = data?.entries ?? []
  const protectedReferences = useMemo(
    () => entries.filter((entry) => !entry.rawItemId).length,
    [entries]
  )
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
      setError(governanceAuditErrorMessage('loadAudit', err))
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
      link.download = `saved-item-change-history-${new Date().toISOString()}.json`
      link.click()
      URL.revokeObjectURL(url)
      setData(response)
      setExportStatus(
        `${response.entries.length} history ${response.entries.length === 1 ? 'row' : 'rows'} exported`
      )
    } catch (err) {
      setError(governanceAuditErrorMessage('exportAudit', err))
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
              Pick a common change view, then narrow it by item, work area, person, or time.
            </p>
          </div>
          <div role="group" aria-label="Common change views" className="grid gap-2 sm:grid-cols-3">
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
          <Field
            label="Change area"
            help="Use the default for normal checks. Paste a change area only when an owner or admin gives you one."
          >
            <input
              data-testid="governance-audit-filter-event-prefix"
              name="eventPrefix"
              autoComplete="off"
              value={filters.eventPrefix}
              onChange={(event) => updateFilter('eventPrefix', event.target.value)}
              placeholder="Paste a change area only when needed"
              className={INPUT_CLASS}
            />
          </Field>
          <Field
            label="Specific change name"
            help="Optional. Use this only when an owner or admin gives you the exact change name."
          >
            <input
              data-testid="governance-audit-filter-event-type"
              name="eventType"
              list="governance-audit-event-type-options"
              autoComplete="off"
              value={filters.eventType}
              onChange={(event) => updateFilter('eventType', event.target.value)}
              placeholder="Pick a view or paste a specific change name"
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
              className={SELECT_CLASS}
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
              className={SELECT_CLASS}
            >
              {SCOPE_KIND_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Rows to show">
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
            <button type="submit" disabled={loading} className={uiStyles.primaryButton}>
              <Search size={15} aria-hidden="true" />
              Show changes
            </button>
            <button
              type="button"
              data-testid="governance-audit-refresh"
              onClick={() => void loadAudit(filters)}
              disabled={loading}
              aria-label="Check change history again"
              className={cn(uiStyles.secondaryButton, 'w-8 px-0')}
              title="Check change history again"
            >
              <RefreshCw size={15} className={cn(loading && 'animate-spin')} aria-hidden="true" />
            </button>
            <button
              type="button"
              data-testid="governance-audit-export"
              onClick={() => void exportAudit()}
              disabled={exporting}
              aria-label="Export change history"
              className={cn(uiStyles.secondaryButton, 'w-8 px-0')}
              title="Export change history"
            >
              <Download size={15} aria-hidden="true" />
            </button>
          </div>
        </div>

        <div className="mt-3 grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_180px_180px_auto]">
          <Field label="Exact work area">
            <input
              value={filters.scopeId}
              name="scopeId"
              autoComplete="off"
              onChange={(event) => updateFilter('scopeId', event.target.value)}
              placeholder="Paste the exact team space, work area, team, or project from Forge only when an owner or admin gives you one"
              className={INPUT_CLASS}
            />
          </Field>
          <Field label="Exact person">
            <input
              value={filters.userId}
              name="userId"
              autoComplete="off"
              onChange={(event) => updateFilter('userId', event.target.value)}
              placeholder="Paste the exact person from Forge only when an owner or admin gives you one"
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
          <div role="alert" aria-live="polite" className={uiStyles.error}>
            {error}
          </div>
        )}
        {exportStatus && (
          <div aria-live="polite" className={cn(uiStyles.note, 'mb-4')}>
            {exportStatus}
          </div>
        )}

        <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
          <Metric label="Changes shown" value={entries.length} />
          <Metric
            label="Selected view"
            value={auditViewMetricLabel(data?.query.eventPrefix ?? filters.eventPrefix)}
            compact
          />
          <Metric label="Protected saved items" value={protectedReferences} />
          <Metric label="Hidden change-note rows" value={redactedRows} />
        </div>

        <div className={uiStyles.card}>
          <div className="overflow-x-auto">
            <table className={cn(uiStyles.table, 'min-w-[1120px]')}>
              <thead className={uiStyles.tableHead}>
                <tr>
                  <th className={uiStyles.tableHeaderCell}>Time</th>
                  <th className={uiStyles.tableHeaderCell}>Change</th>
                  <th className={uiStyles.tableHeaderCell}>Changed item</th>
                  <th className={uiStyles.tableHeaderCell}>Area</th>
                  <th className={uiStyles.tableHeaderCell}>Changed by</th>
                  <th className={uiStyles.tableHeaderCell}>Change check</th>
                  <th className={uiStyles.tableHeaderCell}>Change notes</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-black/5 dark:divide-white/10">
                {loading ? (
                  <tr>
                    <td colSpan={7}>
                      <BeginnerLoadingState
                        framed={false}
                        title="Checking change history"
                        detail="Forge is checking saved note and saved guidance changes for this team space."
                        nextStep="If this takes more than a moment, choose Check change history again or ask an owner or admin to check change history access."
                        success="Success looks like history rows or a Show all change history step."
                      />
                    </td>
                  </tr>
                ) : entries.length === 0 ? (
                  <tr>
                    <td colSpan={7} className={cn(uiStyles.tableCell, 'py-12 text-center')}>
                      <p className="font-semibold text-foreground-light dark:text-foreground-dark">
                        Your filters may be hiding changes
                      </p>
                      <p className="mt-1 text-secondary-light dark:text-secondary-dark">
                        Show all history first, then narrow by item, area, person, or time. If this
                        is a new team space, save a useful instruction or mark a saved note as
                        helpful, then choose Show all change history.
                      </p>
                      <button
                        type="button"
                        onClick={() => applyQuickView(QUICK_AUDIT_VIEWS[0])}
                        className={cn(uiStyles.primaryButton, 'mt-4')}
                      >
                        <Search size={15} aria-hidden="true" />
                        Show all change history
                      </button>
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
        'flex min-h-16 items-center gap-3 rounded-button border px-3 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60',
        active
          ? 'border-black/[0.08] bg-black/[0.06] dark:border-white/[0.1] dark:bg-white/[0.08]'
          : 'border-black/[0.08] bg-white hover:bg-black/[0.04] dark:border-white/[0.1] dark:bg-white/[0.04] dark:hover:bg-white/[0.06]'
      )}
    >
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-button bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
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
      <td className={cn(uiStyles.tableCell, 'w-40 text-secondary-light dark:text-secondary-dark')}>
        <span className="tabular-nums">{formatDate(entry.createdAt)}</span>
      </td>
      <td className={cn(uiStyles.tableCell, 'max-w-[260px]')}>
        <div className="truncate font-medium">{auditEventLabel(entry.eventType)}</div>
        <details className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          <summary className="cursor-pointer select-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/30">
            Show saved change name
          </summary>
          <span className={cn(uiStyles.chip, 'mt-1 max-w-full')}>
            {shortEventType(entry.eventType)}
          </span>
        </details>
        <div className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {auditItemKindLabel(entry.itemKind)} · {resourceTypeLabel(entry.resourceType)}
        </div>
      </td>
      <td className={cn(uiStyles.tableCell, 'w-72')}>
        {entry.rawItemId ? (
          <SubjectLine
            testId="governance-audit-item-reference"
            icon="visible"
            label="Visible saved item"
          />
        ) : (
          <SubjectLine
            testId="governance-audit-protected-reference"
            icon="hash"
            label="Protected saved item"
          />
        )}
        {entry.detailsRedacted && (
          <div data-testid="governance-audit-redacted" className={cn(uiStyles.badge, 'mt-2 gap-1')}>
            <EyeOff size={12} aria-hidden="true" />
            Change notes hidden
          </div>
        )}
      </td>
      <td className={cn(uiStyles.tableCell, 'w-56')}>
        <div className="font-medium">{auditAreaLabel(entry.scopeKind)}</div>
        <div className={cn(uiStyles.chip, 'mt-1 max-w-full truncate')}>
          {entry.scopeId ? `Work area ${shortId(entry.scopeId)}` : 'Work area hidden'}
        </div>
      </td>
      <td className={cn(uiStyles.tableCell, 'w-48')}>
        <span className={cn(uiStyles.chip, 'max-w-full truncate')}>
          {auditActorLabel(entry.actorUserId)}
        </span>
      </td>
      <td className={cn(uiStyles.tableCell, 'w-44')}>
        <TamperBadge status={entry.tamperStatus} />
      </td>
      <td className={cn(uiStyles.tableCell, 'min-w-[260px]')}>
        <details>
          <summary className="cursor-pointer select-none text-ui-caption font-medium text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/30 dark:text-foreground-dark">
            Show change notes
          </summary>
          <pre className="mt-2 max-h-32 overflow-auto rounded-card bg-black/[0.035] p-2 font-mono text-ui-caption leading-relaxed text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark">
            {prettyDetails(entry.details)}
          </pre>
        </details>
      </td>
    </tr>
  )
}

function SubjectLine({
  testId,
  icon,
  label,
}: {
  testId: string
  icon: 'visible' | 'hash'
  label: string
}) {
  const Icon = icon === 'visible' ? ShieldCheck : Fingerprint
  return (
    <div data-testid={testId} className="flex min-w-0 items-center gap-2">
      <Icon size={14} className="shrink-0 text-apple-blue" aria-hidden="true" />
      <span className="shrink-0 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
        {label}
      </span>
    </div>
  )
}

function TamperBadge({ status }: { status: GovernanceAuditTamperStatus }) {
  const config = {
    valid: {
      dot: 'bg-apple-blue',
      label: 'Checked',
    },
    invalid: {
      dot: 'bg-apple-red',
      label: 'Check this change',
    },
    not_configured: {
      dot: 'bg-gray-400',
      label: 'Change check not set up',
    },
  }[status]
  return (
    <span className="inline-flex items-center gap-1.5 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
      <span className={cn('h-1.5 w-1.5 rounded-full', config.dot)} />
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
    <div className={cn(uiStyles.card, 'px-4 py-3')}>
      <div className="text-ui-caption font-semibold text-secondary-light dark:text-secondary-dark">
        {label}
      </div>
      <div
        className={cn(
          'mt-1 truncate font-semibold text-foreground-light dark:text-foreground-dark',
          compact ? 'text-ui-body' : 'text-ui-metric tabular-nums'
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
      <span className={cn(uiStyles.label, 'mb-0')}>{label}</span>
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

function auditEventLabel(eventType: string): string {
  const labels: Record<string, string> = {
    'governance.context.feedback.recorded': 'Feedback saved',
    'governance.context.skill.approved': 'Saved guidance saved for reuse',
    'governance.context.skill.reviewed': 'Saved guidance checked',
    'governance.context.memory.updated': 'Saved note updated',
    'governance.context.memory.rejected': 'Saved note not saved',
  }
  return (
    labels[eventType] ??
    readableCodeLabel(eventType.split('.').slice(-2).join(' '), {
      fallback: 'Check change',
    })
  )
}

function auditViewMetricLabel(eventPrefix: string | undefined): string {
  if (!eventPrefix || eventPrefix === 'governance.context.') return 'All saved item changes'
  if (eventPrefix === 'governance.context.skill.') return 'Saved guidance changes'
  if (eventPrefix === 'governance.context.memory.') return 'Saved note changes'
  return 'Custom change view'
}

function shortEventType(eventType: string): string {
  return eventType.replace(/^governance\.context\./, '').trim() || 'Saved change name missing'
}

function auditItemKindLabel(kind: GovernanceAuditItemKind | null | undefined): string {
  if (kind === 'memory') return 'Saved note'
  if (kind === 'skill') return 'Saved guidance'
  return 'Item hidden for safety'
}

function resourceTypeLabel(value: string): string {
  const normalized = value.trim().toLowerCase()
  if (normalized === 'memory' || normalized === 'memories' || normalized === 'memory_item') {
    return 'Saved note details'
  }
  if (normalized === 'skill' || normalized === 'skills') return 'Guidance details'
  return readableCodeLabel(value, { fallback: 'Check item type' })
}

function auditAreaLabel(kind: GovernanceAuditScopeKind | null | undefined): string {
  switch (kind) {
    case 'org':
      return 'Team space'
    case 'workspace':
      return 'Work area'
    case 'team':
      return 'Team'
    case 'project':
      return 'Project'
    case 'user':
      return 'User account'
    default:
      return 'Area hidden for safety'
  }
}

function auditActorLabel(actorUserId: string | null | undefined): string {
  return actorUserId ? `Team member ${shortId(actorUserId)}` : 'System'
}

function readableCodeLabel(value: string, options: { fallback: string }): string {
  const words = value.replace(/[_-]+/g, ' ').replace(/\s+/g, ' ').trim().toLowerCase()
  if (!words) return options.fallback
  return words.charAt(0).toUpperCase() + words.slice(1)
}

function shortId(value: string): string {
  return value.length <= 22 ? value : `${value.slice(0, 10)}…${value.slice(-8)}`
}

function prettyDetails(value: unknown): string {
  try {
    return JSON.stringify(safeAuditDetailValue(value), null, 2)
  } catch {
    return safeAuditDetailString(String(value))
  }
}

function safeAuditDetailValue(
  value: unknown,
  key = '',
  seen: WeakSet<object> = new WeakSet<object>()
): unknown {
  if (isSensitiveAuditDetailKey(key)) return HIDDEN_AUDIT_DETAIL_VALUE
  if (typeof value === 'string') return safeAuditDetailString(value)
  if (Array.isArray(value)) return value.map((item) => safeAuditDetailValue(item, '', seen))
  if (value && typeof value === 'object') {
    if (seen.has(value)) return REPEATED_AUDIT_DETAIL_VALUE
    seen.add(value)
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([entryKey, entryValue]) => [
        entryKey,
        safeAuditDetailValue(entryValue, entryKey, seen),
      ])
    )
  }
  return value
}

function isSensitiveAuditDetailKey(key: string): boolean {
  const normalizedKey = key.replace(/[^a-z0-9]/gi, '').toLowerCase()
  return ['token', 'secret', 'password', 'apikey', 'credential'].some((sensitivePart) =>
    normalizedKey.includes(sensitivePart)
  )
}

function safeAuditDetailString(value: string): string {
  const accessIssue =
    /\b(missing|invalid|expired|revoked)\b.{0,32}\b(token|credential|credentials|api\s*key|secret)\b/i
  const reversedAccessIssue =
    /\b(token|credential|credentials|api\s*key|secret)\b.{0,32}\b(missing|invalid|expired|revoked)\b/i
  if (accessIssue.test(value) || reversedAccessIssue.test(value)) {
    return MISSING_AUDIT_ACCESS_MESSAGE
  }
  if (/\b(authorization\s*:\s*bearer|bearer\s+[\w.-]{4,})\b/i.test(value)) {
    return HIDDEN_AUDIT_DETAIL_VALUE
  }
  return value
}
