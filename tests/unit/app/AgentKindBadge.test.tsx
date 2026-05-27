import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import { AgentKindBadge } from '@app/features/agents/AgentKindBadge'

afterEach(() => {
  cleanup()
})

describe('AgentKindBadge', () => {
  test('explains local-machine agents without implementation details', () => {
    render(<AgentKindBadge cliTool={'workspace-tool' as never} runtimeKind="host-cli" />)

    const badge = screen.getByText('Host CLI')
    expect(badge).toHaveAttribute(
      'title',
      'Runs on an enrolled computer. Use it when work should stay on that machine.'
    )
  })

  test('explains managed-workspace agents by what they can do', () => {
    render(<AgentKindBadge cliTool={'workspace-tool' as never} />)

    const badge = screen.getByText('Container')
    expect(badge).toHaveAttribute(
      'title',
      'Runs in a managed workspace that can edit files, run commands, and collect evidence.'
    )
  })

  test('explains prompt-only agents by their file access boundary', () => {
    render(<AgentKindBadge />)

    const badge = screen.getByText('Provider')
    expect(badge).toHaveAttribute(
      'title',
      'Handles text-only tasks with a connected model. It does not open workspace files.'
    )
  })
})
