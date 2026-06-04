const RAW_STATUS_ERROR_PATTERN = /\b(?:HTTP|API|Code:)\s*\(?\d{3}\b/i

export const ADMIN_PANEL_RECOVERY =
  'Refresh Admin, then try again. If it still fails, ask an owner to check the admin service and your admin role.'

export const CLI_IMAGE_RECOVERY =
  'Choose Check now again. If it still fails, ask an owner to check the admin service and image updater.'

export function adminPanelLoadErrorMessage(error: string, label: string): string {
  if (!RAW_STATUS_ERROR_PATTERN.test(error)) return error
  return `The admin ${label} could not load.`
}

export function cliImageStatusErrorMessage(error: string): string {
  if (!RAW_STATUS_ERROR_PATTERN.test(error)) return error
  return 'The CLI image status could not load.'
}
