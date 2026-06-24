import { describe, expect, it } from 'vitest'
import {
  LOCAL_AGENT_SETUP_APP_LABEL,
  LOCAL_AGENT_SETUP_WINDOW_LABEL,
  localAgentSetupPasteHint,
} from '@app/entities/agent'

describe('local connection copy', () => {
  it('uses setup-app wording instead of command-window jargon', () => {
    expect(LOCAL_AGENT_SETUP_APP_LABEL).toBe('the setup app shown above')
    expect(LOCAL_AGENT_SETUP_WINDOW_LABEL).toBe('the setup app shown above')
    expect(localAgentSetupPasteHint('posix')).toBe(
      'Open the setup app for macOS or Linux, then paste this setup text.'
    )
    expect(localAgentSetupPasteHint('windows')).toBe(
      'Open the setup app for Windows, then paste this setup text.'
    )

    const visibleCopy = [
      LOCAL_AGENT_SETUP_APP_LABEL,
      LOCAL_AGENT_SETUP_WINDOW_LABEL,
      localAgentSetupPasteHint('posix'),
      localAgentSetupPasteHint('windows'),
    ].join(' ')

    expect(visibleCopy).not.toMatch(/Terminal|PowerShell/)
  })
})
