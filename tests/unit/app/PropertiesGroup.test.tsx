import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { PropertiesGroup } from '@app/features/detail/rail/PropertiesGroup'
import { useContextFeaturesStore } from '@app/entities/context/model/context-features.store'
import { useBoardStore } from '@app/entities/navigation/model/board.store'

const orchestrationApiMock = vi.hoisted(() => ({
  retryTask: vi.fn(),
}))

vi.mock('@app/shared/api/orchestration', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/shared/api/orchestration')>()
  return {
    ...actual,
    orchestrationApi: {
      ...actual.orchestrationApi,
      retryTask: orchestrationApiMock.retryTask,
    },
  }
})

const task = {
  id: 'task-1',
  state: 'queued',
  method: 'work',
  params: { task: 'Test task', message: '' },
  priority: 'normal',
  progress: 0,
  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
  attempt: 1,
} as const

beforeEach(() => {
  orchestrationApiMock.retryTask.mockResolvedValue({ ok: true, task: null })
})

afterEach(() => {
  cleanup()
  useContextFeaturesStore.getState().reset()
  useBoardStore.getState().reset()
  vi.clearAllMocks()
})

describe('PropertiesGroup', () => {
  test('renders the status dot and label while omitting an empty agent row', () => {
    render(<PropertiesGroup task={task} />)

    const statusLabel = screen.getByText('Waiting to start')
    const statusDot = statusLabel.parentElement?.querySelector('[aria-hidden="true"]')
    expect(statusDot?.className).toContain('bg-apple-gray-1')
    expect(screen.queryByText('Agent')).toBeNull()
  })

  test('retries a failed task', async () => {
    render(<PropertiesGroup task={{ ...task, state: 'failed' }} />)

    await userEvent.setup().click(screen.getByRole('button', { name: 'Retry task' }))

    await waitFor(() => expect(orchestrationApiMock.retryTask).toHaveBeenCalledWith('task-1'))
  })
})
