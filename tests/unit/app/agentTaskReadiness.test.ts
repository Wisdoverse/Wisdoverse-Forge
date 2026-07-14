import { describe, expect, it } from 'vitest'

import {
  agentCanTakeTask,
  agentHasTaskCapability,
  agentTaskStatusLabel,
} from '@app/features/board/model/agentTaskReadiness'

describe('agent task readiness', () => {
  it('explains chat-only and missing-status agents in beginner language', () => {
    const chatOnly = { status: 'available', capabilities: [] }
    const missingStatus = { status: ' ' }

    expect(agentHasTaskCapability(chatOnly)).toBe(false)
    expect(agentCanTakeTask(chatOnly)).toBe(false)
    expect(agentTaskStatusLabel(chatOnly)).toBe('questions only - use Chat')
    expect(agentTaskStatusLabel(missingStatus)).toBe('check if ready')
  })
})
