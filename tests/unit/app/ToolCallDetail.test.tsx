import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import { ToolCallDetail } from '@app/features/chat/ToolCallDetail'
import type { ToolCall } from '@app/shared/model/chat.store'

const baseCall: ToolCall = {
  toolUseId: 'tool-1',
  tool: 'shell',
  input: { command: 'npm run typecheck', cwd: '/workspace/app' },
  output: { ok: true, summary: 'Typecheck passed', durationMs: 1200 },
  success: true,
  duration: 1200,
}

afterEach(() => {
  cleanup()
})

describe('ToolCallDetail', () => {
  test('summarizes a completed tool step in operator language', () => {
    render(<ToolCallDetail call={baseCall} />)

    expect(screen.getByText(/Agent recorded a work step/i)).toBeInTheDocument()
    expect(screen.getByText(/Work step: Command runner/i)).toBeInTheDocument()
    expect(screen.queryByText(/Step type: shell/i)).toBeNull()
    expect(screen.getByText('Completed cleanly')).toBeInTheDocument()
    expect(screen.getByText(/This step finished without reporting a problem/i)).toBeInTheDocument()
    expect(screen.getByText('Took 1.2s')).toBeInTheDocument()
    expect(screen.queryByText(/Agent used/i)).toBeNull()
  })

  test('opens beginner summaries before support details', () => {
    render(<ToolCallDetail call={baseCall} />)

    fireEvent.click(screen.getByRole('button', { name: /show step details for command runner/i }))

    expect(
      screen.getByText(
        /this is a read-only record of one step the agent took.*whether to continue, retry, or ask the agent to explain it/i
      )
    ).toBeInTheDocument()
    expect(screen.getByText('Step setup')).toBeInTheDocument()
    expect(
      screen.getByText('Settings or instructions recorded for this step.')
    ).toBeInTheDocument()
    expect(screen.getByText('Step result')).toBeInTheDocument()
    expect(screen.getByText('What happened when this step finished.')).toBeInTheDocument()
    expect(screen.getByText('Command the agent used: npm run typecheck')).toBeInTheDocument()
    expect(screen.getByText(/Typecheck passed/)).toBeInTheDocument()
    expect(screen.queryByText(/cwd/i)).toBeNull()
    expect(screen.queryByText(/durationMs/i)).toBeNull()
    expect(screen.queryByText(/What the agent sent/i)).toBeNull()
    expect(screen.queryByText(/What came back/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /show support details for setup/i }))
    fireEvent.click(screen.getByRole('button', { name: /show support details for result/i }))

    expect(screen.getByText(/Folder: \/workspace\/app/i)).toBeInTheDocument()
    expect(screen.getByText(/Duration: 1.2s/i)).toBeInTheDocument()
    expect(screen.queryByText(/cwd/i)).toBeNull()
    expect(screen.queryByText(/durationMs/i)).toBeNull()
  })

  test('flags failed tool output as something to review before trusting the answer', () => {
    render(
      <ToolCallDetail
        call={{
          ...baseCall,
          tool: 'deploy',
          input: { target: 'preview', token: 'secret-token-value' },
          output: { error: 'Missing token' },
          success: false,
          duration: 400,
        }}
      />
    )

    expect(screen.getByText('Needs review')).toBeInTheDocument()
    expect(screen.getByText(/This step reported a problem/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /show step details for deployment/i }))

    expect(
      screen.getByText('Review this result before relying on the final answer.')
    ).toBeInTheDocument()
    expect(screen.getByText(/Required account access is missing/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /show support details for setup/i }))

    expect(screen.getByText(/Hidden for safety/i)).toBeInTheDocument()
    expect(screen.getByText(/Account access:/i)).toBeInTheDocument()
    expect(screen.queryByText(/Missing token/i)).toBeNull()
    expect(screen.queryByText(/token:/i)).toBeNull()
    expect(screen.queryByText(/secret-token-value/i)).toBeNull()
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
    expect(screen.getByText(/no result has been recorded yet/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /show step details for search/i }))

    expect(screen.getByText('No result has been recorded for this step yet.')).toBeInTheDocument()
  })

  test('turns unknown tool slugs into readable step names', () => {
    render(
      <ToolCallDetail
        call={{
          ...baseCall,
          tool: 'future_tool_runner',
          input: { summary: 'Checked the release branch' },
        }}
      />
    )

    expect(screen.getByText(/Work step: Future Tool Runner/i)).toBeInTheDocument()
    expect(screen.queryByText(/future_tool_runner/i)).toBeNull()
  })
})
