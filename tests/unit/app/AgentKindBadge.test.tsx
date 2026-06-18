import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import { AgentKindBadge } from '@app/features/agents/AgentKindBadge'

afterEach(() => {
  cleanup()
})

describe('AgentKindBadge', () => {
  test('explains local-machine agents without implementation details', () => {
    render(<AgentKindBadge cliTool={'workspace-tool' as never} runtimeKind="cli" />)

    const badge = screen.getByText('This computer')
    expect(badge).toHaveAttribute(
      'title',
      'Uses files and tools on this connected computer. Use it when work should stay there.'
    )
  })

  test('explains project-file agents by what they can do', () => {
    render(<AgentKindBadge cliTool={'workspace-tool' as never} />)

    const badge = screen.getByText('Project files')
    expect(badge).toHaveAttribute(
      'title',
      'Works with shared project files. It can change files, run checks, and save what it checked.'
    )
  })

  test('explains chat-only agents by their file access boundary', () => {
    render(<AgentKindBadge />)

    const badge = screen.getByText('Chat-only')
    expect(badge).toHaveAttribute(
      'title',
      'Answers in chat through a connected AI service. It cannot open project files on its own.'
    )
  })
})
