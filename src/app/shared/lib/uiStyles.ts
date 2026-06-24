export const uiStyles = {
  sectionHeader: 'mb-4 flex items-center justify-between gap-3',
  sectionTitle: 'text-ui-title font-semibold text-foreground-light dark:text-foreground-dark',
  sectionDescription: 'mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark',
  groupLabel:
    'mb-2 text-ui-caption font-semibold uppercase text-secondary-light dark:text-secondary-dark',
  card: 'overflow-hidden rounded-card border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-[#2c2c2e]',
  cardPadded:
    'rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2c2c2e]',
  row: 'border-b border-black/[0.06] last:border-b-0 dark:border-white/[0.08]',
  table: 'w-full min-w-[640px] text-left text-ui-caption',
  tableHead:
    'border-b border-black/[0.06] bg-black/[0.025] text-ui-caption text-secondary-light dark:border-white/[0.08] dark:bg-white/[0.03] dark:text-secondary-dark',
  tableHeaderCell: 'px-4 py-2.5 font-medium',
  tableCell: 'px-4 py-3',
  label: 'mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark',
  input:
    'h-9 w-full rounded-full border border-black/[0.08] bg-white px-3 text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70',
  select:
    'h-9 rounded-full border border-black/[0.08] bg-white px-3 text-ui-body text-foreground-light outline-none transition-colors focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark',
  primaryButton:
    'inline-flex h-9 items-center justify-center gap-1.5 rounded-full bg-apple-blue px-3 text-ui-button font-semibold text-white transition-colors hover:bg-apple-blue-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60',
  secondaryButton:
    'inline-flex h-9 items-center justify-center gap-1.5 rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.03] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.08]',
  subtleButton:
    'inline-flex h-9 items-center justify-center gap-1.5 rounded-full px-3 text-ui-button font-medium text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark',
  dangerButton:
    'inline-flex h-8 items-center justify-center rounded-full px-3 text-ui-caption font-semibold text-apple-red transition-colors hover:bg-apple-red/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/30 disabled:cursor-not-allowed disabled:opacity-60',
  dangerConfirmButton:
    'inline-flex h-8 items-center justify-center rounded-full bg-apple-red px-3 text-ui-caption font-semibold text-white transition-colors hover:bg-apple-red/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/30 disabled:cursor-not-allowed disabled:opacity-60',
  badge:
    'inline-flex items-center rounded-full border border-black/[0.08] bg-white px-2 py-0.5 text-ui-caption font-medium text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark',
  activeBadge:
    'inline-flex items-center rounded-full bg-apple-blue/10 px-2 py-0.5 text-ui-caption font-semibold text-apple-blue',
  error:
    'mb-4 rounded-card border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red',
  note: 'rounded-card border border-black/[0.08] bg-black/[0.025] px-3 py-2 text-ui-body text-secondary-light dark:border-white/[0.08] dark:bg-white/[0.03] dark:text-secondary-dark',
}
