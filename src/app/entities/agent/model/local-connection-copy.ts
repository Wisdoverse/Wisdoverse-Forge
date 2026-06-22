export const LOCAL_AGENT_SETUP_APP_LABEL = 'Terminal or PowerShell'
export const LOCAL_AGENT_SETUP_WINDOW_LABEL = 'Terminal on macOS/Linux or PowerShell on Windows'

export function localAgentSetupPasteHint(os: 'posix' | 'windows'): string {
  return os === 'windows'
    ? 'Open PowerShell on Windows, then paste this setup text.'
    : 'Open Terminal on macOS/Linux, then paste this setup text.'
}
