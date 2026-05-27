import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { RightPanel } from '@app/layouts/RightPanel'
import { useBoardStore } from '@app/shared/model/board.store'

afterEach(() => {
  cleanup()
  useBoardStore.getState().reset()
})

describe('RightPanel', () => {
  test('labels the default panel as live task updates for new users', () => {
    render(
      <RightPanel collapsed={false} onToggle={() => {}}>
        <div>Panel content</div>
      </RightPanel>
    )

    expect(screen.getByRole('heading', { name: /live task updates/i })).toBeDefined()
    expect(screen.getByText(/agent progress, blockers, and finished work/i)).toBeDefined()
    expect(screen.getByLabelText(/hide live task updates panel/i)).toBeDefined()
  })

  test('calls the toggle handler from the readable hide control', async () => {
    const onToggle = vi.fn()
    const user = userEvent.setup()

    render(
      <RightPanel collapsed={false} onToggle={onToggle}>
        <div>Panel content</div>
      </RightPanel>
    )

    await user.click(screen.getByLabelText(/hide live task updates panel/i))

    expect(onToggle).toHaveBeenCalledTimes(1)
  })
})
