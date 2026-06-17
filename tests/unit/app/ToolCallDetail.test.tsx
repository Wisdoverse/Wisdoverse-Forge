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
    expect(screen.getByText(/This step finished without a problem/i)).toBeInTheDocument()
    expect(screen.queryByText(/without reporting a problem/i)).toBeNull()
    expect(screen.getByText('Took 1.2s')).toBeInTheDocument()
    expect(screen.queryByText(/Agent used/i)).toBeNull()
  })

  test('opens beginner summaries before extra details', () => {
    render(<ToolCallDetail call={baseCall} />)

    fireEvent.click(screen.getByRole('button', { name: /show step details for command runner/i }))

    expect(
      screen.getByText(
        /this is a read-only record of one step the agent took.*whether to continue, retry, or ask the agent to explain it/i
      )
    ).toBeInTheDocument()
    expect(screen.getByText('Before this step')).toBeInTheDocument()
    expect(
      screen.getByText('What the agent was told or given before it ran this step.')
    ).toBeInTheDocument()
    expect(screen.getByText('After this step')).toBeInTheDocument()
    expect(
      screen.getByText('What the agent reported after this step finished.')
    ).toBeInTheDocument()
    expect(screen.getByText('Command the agent used: npm run typecheck')).toBeInTheDocument()
    expect(screen.getByText(/Typecheck passed/)).toBeInTheDocument()
    expect(screen.queryByText(/cwd/i)).toBeNull()
    expect(screen.queryByText(/durationMs/i)).toBeNull()
    expect(screen.queryByText(/What the agent sent/i)).toBeNull()
    expect(screen.queryByText(/What came back/i)).toBeNull()
    expect(screen.queryByText(/support details/i)).toBeNull()
    expect(screen.queryByText(/support review/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /show setup details/i }))
    fireEvent.click(screen.getByRole('button', { name: /show result details/i }))

    expect(screen.getByText(/Project folder: \/workspace\/app/i)).toBeInTheDocument()
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

    expect(screen.getByText('Check step')).toBeInTheDocument()
    expect(screen.queryByText('Needs review')).toBeNull()
    expect(screen.getByText(/This step found a problem/i)).toBeInTheDocument()
    expect(screen.queryByText(/This step reported a problem/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /show step details for deployment/i }))

    expect(
      screen.getByText('Review this result before relying on the final answer.')
    ).toBeInTheDocument()
    expect(screen.getByText(/Required account access is missing/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /show setup details/i }))

    expect(screen.getByText(/Hidden for safety/i)).toBeInTheDocument()
    expect(screen.getByText(/Account access:/i)).toBeInTheDocument()
    expect(screen.queryByText(/Missing token/i)).toBeNull()
    expect(screen.queryByText(/token:/i)).toBeNull()
    expect(screen.queryByText(/secret-token-value/i)).toBeNull()
  })

  test('hides technical tool failure details from summaries and extra details', () => {
    render(
      <ToolCallDetail
        call={{
          ...baseCall,
          output: {
            error: 'panic: stack trace line 7\nsecret token abc\nraw command output',
          },
          success: false,
        }}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: /show step details for command runner/i }))

    expect(
      screen.getByText(
        /Check this step: This step hit a problem\. Ask the agent to explain what happened, then retry if the task still matters\./i
      )
    ).toBeInTheDocument()
    expect(screen.queryByText(/Needs attention/i)).toBeNull()
    expect(screen.queryByText(/panic/i)).toBeNull()
    expect(screen.queryByText(/stack trace/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
    expect(screen.queryByText(/raw command output/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /show result details/i }))

    expect(screen.getByText(/Problem: This step hit a problem/i)).toBeInTheDocument()
    expect(screen.queryByText(/technical problem/i)).toBeNull()
    expect(screen.queryByText(/panic/i)).toBeNull()
    expect(screen.queryByText(/stack trace/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
    expect(screen.queryByText(/raw command output/i)).toBeNull()
  })

  test('turns failed boolean results into an action before users trust the answer', () => {
    render(
      <ToolCallDetail
        call={{
          ...baseCall,
          output: { ok: false },
          success: false,
        }}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: /show step details for command runner/i }))

    expect(screen.getByText('Check this step before relying on the answer.')).toBeInTheDocument()
    expect(screen.queryByText('This step needs review.')).toBeNull()
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
    expect(screen.getByText(/wait for it to share what happened/i)).toBeInTheDocument()
    expect(screen.queryByText(/wait for it to report what happened/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /show step details for search/i }))

    expect(screen.getByText(/this step does not have a result yet/i)).toBeInTheDocument()
    expect(screen.getByText(/wait for another update/i)).toBeInTheDocument()
    expect(screen.queryByText(/has not reported a result yet/i)).toBeNull()
    expect(screen.queryByText(/No result has been recorded/i)).toBeNull()
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
