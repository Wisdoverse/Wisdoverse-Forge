import { describe, expect, test } from 'vitest'
import { render, screen } from '@testing-library/react'
import { TurnItem } from '@app/features/chat/TurnItem'
import type { Turn } from '@app/shared/model/chat.store'

const baseTurn: Turn = {
  id: 'turn-1',
  prompt: 'Can you deploy staging?',
  toolCalls: [],
  response: 'I will check the deployment status first.',
  timestamp: new Date('2026-05-25T12:00:00.000Z').getTime(),
}

describe('TurnItem', () => {
  test('shows readable speaker labels for the operator and agent', () => {
    render(<TurnItem turn={baseTurn} />)

    expect(screen.getByText('You')).toBeDefined()
    expect(screen.getByText('Agent')).toBeDefined()
    expect(screen.queryByText('U')).toBeNull()
    expect(screen.queryByText('A')).toBeNull()
    expect(screen.getByLabelText('Your message')).toHaveTextContent('Can you deploy staging?')
    expect(screen.getByLabelText('Agent response')).toHaveTextContent(
      'I will check the deployment status first.'
    )
  })

  test('labels tool calls as agent-used tools within the turn', () => {
    render(
      <TurnItem
        turn={{
          ...baseTurn,
          toolCalls: [
            {
              toolUseId: 'tool-1',
              tool: 'check_deployment',
              input: { environment: 'staging' },
              output: { status: 'healthy' },
              success: true,
            },
          ],
        }}
      />
    )

    expect(screen.getByLabelText('Tools used by the agent')).toBeDefined()
    expect(
      screen.getByText(
        /the agent used tools during this turn.*what it sent and what came back before choosing the next step/i
      )
    ).toBeDefined()
    expect(screen.getByText('check_deployment')).toBeDefined()
  })
})
