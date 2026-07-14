import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { BillingInvoice, InvoiceStatus } from '@app/shared/api/legacy/billingApi'

// ============================================================================
// Helpers
// ============================================================================

function formatCurrency(amount: number, currency: string): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: currency.toUpperCase(),
    minimumFractionDigits: 2,
  }).format(amount / 100)
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}

function statusConfig(status: InvoiceStatus): {
  label: string
  description: string
  dot: string
} {
  switch (status) {
    case 'paid':
      return {
        label: 'Paid',
        description: 'No action needed.',
        dot: 'bg-apple-blue',
      }
    case 'open':
      return {
        label: 'Payment due',
        description: 'Pay this invoice to keep your plan active.',
        dot: 'bg-apple-blue',
      }
    case 'void':
      return {
        label: 'Canceled',
        description: 'This invoice was voided and no payment is needed.',
        dot: 'bg-gray-400',
      }
    case 'draft':
      return {
        label: 'Preparing',
        description: 'This invoice is still being prepared.',
        dot: 'bg-gray-400',
      }
    case 'uncollectible':
      return {
        label: 'Payment failed',
        description: 'Update your payment method to resolve this invoice.',
        dot: 'bg-apple-red',
      }
  }
}

// ============================================================================
// InvoiceList
// ============================================================================

interface InvoiceListProps {
  invoices: BillingInvoice[]
  loading?: boolean
  error?: string | null
  retrying?: boolean
  onRetry?: () => void
}

export function InvoiceList({ invoices, loading, error, retrying, onRetry }: InvoiceListProps) {
  return (
    <div>
      <h2 className="mb-4 text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
        Invoices and receipts
      </h2>

      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {loading && (
          <div className="px-6 py-8 flex flex-col gap-3">
            {[0, 1, 2].map((i) => (
              <div key={i} className="flex items-center gap-4 animate-pulse">
                <div className="h-4 w-28 bg-black/10 dark:bg-white/10 rounded" />
                <div className="h-4 w-20 bg-black/10 dark:bg-white/10 rounded" />
                <div className="h-4 w-16 bg-black/10 dark:bg-white/10 rounded ml-auto" />
              </div>
            ))}
          </div>
        )}

        {!loading && error && (
          <div role="alert" aria-live="polite" className="px-6 py-8 text-center">
            <p className="text-ui-body text-apple-red">{error}</p>
            <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Choose Check billing again to load invoices. If it still fails, ask an owner or admin
              to check billing access.
            </p>
            {onRetry && (
              <button
                type="button"
                onClick={onRetry}
                disabled={retrying}
                className={cn(uiStyles.secondaryButton, 'mt-3')}
              >
                {retrying ? 'Checking billing' : 'Check billing again'}
              </button>
            )}
          </div>
        )}

        {!loading && !error && invoices.length === 0 && (
          <div className="px-6 py-8 text-center">
            <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              Start or change a plan to create the first invoice
            </p>
            <p className="mx-auto mt-1 max-w-sm text-ui-caption text-secondary-light dark:text-secondary-dark">
              After a charge is created, return here to open the payment link or download the
              receipt.
            </p>
          </div>
        )}

        {!loading && !error && invoices.length > 0 && (
          <p className="px-4 pb-2 pt-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Invoices appear after you start or change a plan and a charge is created.
          </p>
        )}

        {!loading && !error && invoices.length > 0 && (
          <table className={uiStyles.table}>
            <thead className={uiStyles.tableHead}>
              <tr>
                <th className={uiStyles.tableHeaderCell}>Date</th>
                <th className={uiStyles.tableHeaderCell}>Invoice</th>
                <th className={uiStyles.tableHeaderCell}>Status</th>
                <th className={cn(uiStyles.tableHeaderCell, 'text-right')}>Amount</th>
                <th className={cn(uiStyles.tableHeaderCell, 'text-right')}>Receipt</th>
              </tr>
            </thead>
            <tbody>
              {invoices.map((inv) => {
                const { label, description, dot } = statusConfig(inv.status)
                return (
                  <tr
                    key={inv.id}
                    className="border-b border-black/[0.06] transition-colors hover:bg-black/[0.02] dark:border-white/[0.08] dark:hover:bg-white/[0.02]"
                  >
                    <td
                      className={cn(
                        uiStyles.tableCell,
                        'text-foreground-light dark:text-foreground-dark'
                      )}
                    >
                      {formatDate(inv.createdAt)}
                    </td>
                    <td className={uiStyles.tableCell}>
                      <span className={uiStyles.chip}>{inv.number ?? inv.id.slice(0, 12)}</span>
                    </td>
                    <td className={uiStyles.tableCell}>
                      <div className="flex flex-col gap-1">
                        <span className="inline-flex w-fit items-center gap-1.5 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                          <span className={cn('h-1.5 w-1.5 rounded-full', dot)} />
                          {label}
                        </span>
                        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                          {description}
                        </span>
                      </div>
                    </td>
                    <td
                      className={cn(
                        uiStyles.tableCell,
                        'text-right font-medium text-foreground-light dark:text-foreground-dark'
                      )}
                    >
                      {formatCurrency(inv.total, inv.currency)}
                    </td>
                    <td className={cn(uiStyles.tableCell, 'text-right')}>
                      {inv.pdfUrl ? (
                        <a
                          href={inv.pdfUrl}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-ui-caption text-apple-blue hover:underline"
                        >
                          Download
                        </a>
                      ) : inv.hostedInvoiceUrl ? (
                        <a
                          href={inv.hostedInvoiceUrl}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-ui-caption text-apple-blue hover:underline"
                        >
                          Open
                        </a>
                      ) : (
                        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                          Receipt appears after payment finishes
                        </span>
                      )}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
