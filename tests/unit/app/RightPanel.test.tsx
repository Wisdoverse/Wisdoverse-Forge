import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { RightPanel } from '@app/layouts/RightPanel'
import { useBoardStore } from '@app/entities/navigation/model/board.store'

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
    expect(
      screen.getByText(/agent progress, help needed, and finished task results/i)
    ).toBeDefined()
    expect(screen.queryByText(/agent progress, help needed, and finished work/i)).toBeNull()
    expect(screen.queryByText(/blockers/i)).toBeNull()
    expect(screen.getByLabelText(/hide live task updates/i)).toBeDefined()
    expect(screen.queryByLabelText(/panel/i)).toBeNull()
    const panel = screen.getByTestId('right-panel')
    expect(panel).toHaveClass('min-h-0', 'overflow-hidden')
    expect(panel.className).not.toContain('backdrop-blur')
  })

  test('calls the toggle handler from the readable hide control', async () => {
    const onToggle = vi.fn()
    const user = userEvent.setup()

    render(
      <RightPanel collapsed={false} onToggle={onToggle}>
        <div>Panel content</div>
      </RightPanel>
    )

    await user.click(screen.getByLabelText(/hide live task updates/i))

    expect(onToggle).toHaveBeenCalledTimes(1)
  })
})
