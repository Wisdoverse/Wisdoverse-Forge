interface AccessDateLabels {
  missing: string
  invalid: string
}

export function formatAccessDate(
  dateStr: string | null | undefined,
  labels: AccessDateLabels
): string {
  if (!dateStr) return labels.missing

  const date = new Date(dateStr)
  if (Number.isNaN(date.getTime())) return labels.invalid

  return date.toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}
