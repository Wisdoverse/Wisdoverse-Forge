export const uiStyles = {
  sectionHeader: 'mb-4 flex items-center justify-between gap-3',
  sectionTitle: 'text-ui-title font-semibold text-foreground-light dark:text-foreground-dark',
  sectionDescription: 'mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark',
  groupLabel: 'mb-2 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark',
  card: 'overflow-hidden rounded-card border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-surface-dark',
  cardPadded:
    'rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-surface-dark',
  row: 'border-b border-black/[0.06] last:border-b-0 dark:border-white/[0.08]',
  table: 'w-full min-w-[640px] text-left text-ui-body',
  tableHead:
    'border-b border-black/[0.06] text-ui-caption font-medium text-secondary-light dark:border-white/[0.08] dark:text-secondary-dark',
  tableHeaderCell: 'px-4 py-2 font-medium',
  tableCell: 'px-4 py-2.5',
  label: 'mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark',
  input:
    'h-8 w-full rounded-button border border-black/[0.08] bg-white px-2.5 text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light/70 focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark/70',
  select:
    'h-8 rounded-button border border-black/[0.08] bg-white px-2.5 text-ui-body text-foreground-light outline-none transition-colors focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark',
  primaryButton:
    'inline-flex h-8 items-center justify-center gap-1.5 rounded-button bg-apple-blue px-3 text-ui-button font-medium text-white transition-colors hover:bg-apple-blue-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60',
  secondaryButton:
    'inline-flex h-8 items-center justify-center gap-1.5 rounded-button border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.03] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.08]',
  subtleButton:
    'inline-flex h-8 items-center justify-center gap-1.5 rounded-button px-2.5 text-ui-button font-medium text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus disabled:cursor-not-allowed disabled:opacity-60 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark',
  dangerButton:
    'inline-flex h-8 items-center justify-center rounded-button px-2.5 text-ui-button font-medium text-apple-red transition-colors hover:bg-apple-red/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/30 disabled:cursor-not-allowed disabled:opacity-60',
  dangerConfirmButton:
    'inline-flex h-8 items-center justify-center rounded-button bg-apple-red px-2.5 text-ui-button font-medium text-white transition-colors hover:bg-apple-red/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/30 disabled:cursor-not-allowed disabled:opacity-60',
  badge:
    'inline-flex items-center rounded-button border border-black/[0.08] px-1.5 py-0.5 text-ui-caption font-medium text-secondary-light dark:border-white/[0.1] dark:text-secondary-dark',
  activeBadge:
    'inline-flex items-center rounded-button bg-apple-blue/10 px-1.5 py-0.5 text-ui-caption font-medium text-apple-blue',
  chip: 'inline-flex items-center rounded bg-black/[0.05] px-1.5 py-0.5 font-mono text-ui-caption text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark',
  error:
    'mb-4 rounded-card border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red',
  note: 'rounded-card border border-black/[0.08] bg-black/[0.025] px-3 py-2 text-ui-body text-secondary-light dark:border-white/[0.08] dark:bg-white/[0.03] dark:text-secondary-dark',
}
