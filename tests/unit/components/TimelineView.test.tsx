import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { TimelineView } from '@app/widgets/views/TimelineView'
import { useBoardStore } from '@app/shared/model/board.store'

const canvasContext = {
  beginPath: vi.fn(),
  clearRect: vi.fn(),
  fillRect: vi.fn(),
  fillText: vi.fn(),
  lineTo: vi.fn(),
  moveTo: vi.fn(),
  setTransform: vi.fn(),
  stroke: vi.fn(),
}

beforeEach(() => {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(() => canvasContext)
  vi.spyOn(HTMLCanvasElement.prototype, 'getBoundingClientRect').mockImplementation(
    () =>
      ({
        bottom: 320,
        height: 320,
        left: 0,
        right: 640,
        top: 0,
        width: 640,
        x: 0,
        y: 0,
        toJSON: () => ({}),
      }) as DOMRect
  )
})

afterEach(() => {
  cleanup()
  useBoardStore.getState().reset()
  vi.restoreAllMocks()
})

describe('TimelineView', () => {
  test('shows a beginner-friendly empty timeline state', () => {
    render(<TimelineView />)

    expect(screen.getByTestId('timeline-view')).toBeDefined()
    expect(screen.getByText('Open the task board to start the timeline')).toBeDefined()
    expect(
      screen.getByText(
        'Create a task or open one that is already running. Timeline updates appear here after work starts.'
      )
    ).toBeDefined()
    expect(screen.getByText('Choose Open task board')).toBeDefined()
    expect(
      screen.getByText('Create a small task or open one that is already running')
    ).toBeDefined()
    expect(
      screen.getByText(
        'Return to Timeline to see waiting, working, help needed, and finished updates'
      )
    ).toBeDefined()
    expect(screen.getByRole('button', { name: 'Open task board' })).toBeDefined()
    expect(screen.queryByText('Start a task to build the timeline')).toBeNull()
    expect(screen.queryByText(/something that needs attention/i)).toBeNull()
    expect(screen.queryByText('No timeline events yet')).toBeNull()
    expect(screen.queryByText(/blocked and completed/i)).toBeNull()
  })

  test('lets beginners go back to the task board from the empty timeline', () => {
    useBoardStore.getState().setViewMode('timeline')

    render(<TimelineView />)

    fireEvent.click(screen.getByRole('button', { name: 'Open task board' }))

    expect(useBoardStore.getState().viewMode).toBe('board')
  })

  test('keeps the timeline canvas mounted for the route smoke test', () => {
    render(<TimelineView />)

    expect(document.querySelector('canvas.timeline-canvas')).toBeTruthy()
    expect(canvasContext.fillText).toHaveBeenCalledWith('Waiting for task updates', 320, 136)
    expect(canvasContext.fillText).not.toHaveBeenCalledWith('Waiting for run events', 320, 136)
  })
})
