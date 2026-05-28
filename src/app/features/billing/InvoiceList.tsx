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
  color: string
} {
  switch (status) {
    case 'paid':
      return {
        label: 'Paid',
        description: 'No action needed.',
        color: 'bg-apple-blue/10 text-apple-blue',
      }
    case 'open':
      return {
        label: 'Payment due',
        description: 'Pay this invoice to keep your plan active.',
        color: 'bg-apple-blue/10 text-apple-blue',
      }
    case 'void':
      return {
        label: 'Canceled',
        description: 'This invoice was voided and no payment is needed.',
        color: 'text-secondary-light bg-black/5 dark:bg-white/5 dark:text-secondary-dark',
      }
    case 'draft':
      return {
        label: 'Preparing',
        description: 'This invoice is still being prepared.',
        color: 'text-secondary-light bg-black/5 dark:bg-white/5 dark:text-secondary-dark',
      }
    case 'uncollectible':
      return {
        label: 'Payment failed',
        description: 'Update your payment method to resolve this invoice.',
        color: 'bg-apple-red/10 text-apple-red',
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
}

export function InvoiceList({ invoices, loading, error }: InvoiceListProps) {
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
          <div role="alert" className="px-6 py-8 text-center">
            <p className="text-ui-body text-apple-red">{error}</p>
            <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Try again later or ask an administrator to check billing access.
            </p>
          </div>
        )}

        {!loading && !error && invoices.length === 0 && (
          <div className="px-6 py-8 text-center">
            <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              No invoices have been created yet
            </p>
            <p className="mx-auto mt-1 max-w-sm text-ui-caption text-secondary-light dark:text-secondary-dark">
              Receipts and payment links will appear here after the first billing cycle.
            </p>
            <p className="mx-auto mt-1 max-w-sm text-ui-caption text-secondary-light dark:text-secondary-dark">
              Invoices appear after checkout or a billing portal change creates a charge.
            </p>
          </div>
        )}

        {!loading && !error && invoices.length > 0 && (
          <p className="px-4 pb-2 pt-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Invoices appear after checkout or a billing portal change creates a charge.
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
            <tbody className="divide-y divide-[rgb(var(--border))]">
              {invoices.map((inv) => {
                const { label, description, color } = statusConfig(inv.status)
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
                    <td
                      className={cn(
                        uiStyles.tableCell,
                        'font-mono text-ui-caption text-secondary-light dark:text-secondary-dark'
                      )}
                    >
                      {inv.number ?? inv.id.slice(0, 12)}
                    </td>
                    <td className={uiStyles.tableCell}>
                      <div className="flex flex-col gap-1">
                        <span
                          className={cn(
                            'inline-flex w-fit items-center rounded-full px-2 py-0.5 text-ui-caption font-medium',
                            color
                          )}
                        >
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
                          No link
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
