export const LOCAL_AGENT_SETUP_APP_LABEL = 'the setup app shown above'
export const LOCAL_AGENT_SETUP_WINDOW_LABEL = 'the setup app shown above'

export function localAgentSetupPasteHint(os: 'posix' | 'windows'): string {
  return os === 'windows'
    ? 'Open the setup app for Windows, then paste this setup text.'
    : 'Open the setup app for macOS or Linux, then paste this setup text.'
}
