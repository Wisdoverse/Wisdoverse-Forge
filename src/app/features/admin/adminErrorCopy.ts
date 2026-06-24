const RAW_STATUS_ERROR_PATTERN = /\b(?:HTTP|API|Code:)\s*\(?\d{3}\b/i
const RAW_BACKEND_DETAIL_PATTERN =
  /\b(database|sql|stack trace|traceback|exception|panic|internal server error)\b/i

export const ADMIN_PANEL_RECOVERY =
  'Open Admin again, then choose this section. If it still fails, ask an owner or admin to check your Admin access and this Admin page.'

export const CLI_IMAGE_RECOVERY =
  'Choose Check now again. If it still fails, ask an owner or admin to check Tool updates in Admin.'

export function adminPanelLoadErrorMessage(error: string, label: string): string {
  if (!RAW_STATUS_ERROR_PATTERN.test(error) && !RAW_BACKEND_DETAIL_PATTERN.test(error)) return error
  return `Open Admin again, then choose ${label}.`
}

export function cliImageStatusErrorMessage(error: string): string {
  if (!RAW_STATUS_ERROR_PATTERN.test(error) && !RAW_BACKEND_DETAIL_PATTERN.test(error)) return error
  return 'Choose Check now to load tool update status.'
}
