import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import { ToolCallDetail } from '@app/features/chat/ToolCallDetail'
import type { ToolCall } from '@app/shared/model/chat.store'

const baseCall: ToolCall = {
  toolUseId: 'tool-1',
  tool: 'shell',
  input: { command: 'npm run typecheck' },
  output: { ok: true, summary: 'Typecheck passed' },
  success: true,
  duration: 1200,
}

afterEach(() => {
  cleanup()
})

describe('ToolCallDetail', () => {
  test('summarizes a completed tool step in operator language', () => {
    render(<ToolCallDetail call={baseCall} />)

    expect(screen.getByText(/Agent used/i)).toBeInTheDocument()
    expect(screen.getByText('shell')).toBeInTheDocument()
    expect(screen.getByText('Completed cleanly')).toBeInTheDocument()
    expect(screen.getByText('The tool finished without reporting a problem.')).toBeInTheDocument()
    expect(screen.getByText('Took 1.2s')).toBeInTheDocument()
  })

  test('opens beginner labels before raw input and result details', () => {
    render(<ToolCallDetail call={baseCall} />)

    fireEvent.click(screen.getByRole('button', { name: /show details for shell/i }))

    expect(screen.getByText('What the agent sent')).toBeInTheDocument()
    expect(screen.getByText('Settings or instructions passed into this step.')).toBeInTheDocument()
    expect(screen.getByText('What came back')).toBeInTheDocument()
    expect(screen.getByText('Result returned by the tool.')).toBeInTheDocument()
    expect(screen.getByText(/npm run typecheck/)).toBeInTheDocument()
    expect(screen.getByText(/Typecheck passed/)).toBeInTheDocument()
  })

  test('flags failed tool output as something to review before trusting the answer', () => {
    render(
      <ToolCallDetail
        call={{
          ...baseCall,
          tool: 'deploy',
          input: { target: 'preview' },
          output: { error: 'Missing token' },
          success: false,
          duration: 400,
        }}
      />
    )

    expect(screen.getByText('Needs review')).toBeInTheDocument()
    expect(
      screen.getByText('The tool reported a problem. Check this before trusting the answer.')
    ).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /show details for deploy/i }))

    expect(
      screen.getByText('Review this result before relying on the final answer.')
    ).toBeInTheDocument()
    expect(screen.getByText(/Missing token/)).toBeInTheDocument()
  })

  test('explains when a tool step has not returned a result yet', () => {
    render(
      <ToolCallDetail
        call={{
          toolUseId: 'tool-pending',
          tool: 'search',
          input: { query: 'release notes' },
        }}
      />
    )

    expect(screen.getByText('Waiting for result')).toBeInTheDocument()
    expect(
      screen.getByText('The agent started this step, but no result has been recorded yet.')
    ).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /show details for search/i }))

    expect(screen.getByText('No result has been recorded for this step yet.')).toBeInTheDocument()
  })
})
