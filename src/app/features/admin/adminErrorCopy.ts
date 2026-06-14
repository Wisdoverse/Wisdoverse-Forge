const RAW_STATUS_ERROR_PATTERN = /\b(?:HTTP|API|Code:)\s*\(?\d{3}\b/i

export const ADMIN_PANEL_RECOVERY =
  'Refresh Admin, then try again. If it still fails, ask an owner or admin to check Admin setup and your role.'

export const CLI_IMAGE_RECOVERY =
  'Choose Check now again. If it still fails, ask an owner or admin to check tool update setup.'

export function adminPanelLoadErrorMessage(error: string, label: string): string {
  if (!RAW_STATUS_ERROR_PATTERN.test(error)) return error
  return `Refresh Admin to reload the ${label}.`
}

export function cliImageStatusErrorMessage(error: string): string {
  if (!RAW_STATUS_ERROR_PATTERN.test(error)) return error
  return 'Choose Check now to load tool update status.'
}
