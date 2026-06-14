import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { TimelineView } from '@app/widgets/views/TimelineView'

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
  vi.restoreAllMocks()
})

describe('TimelineView', () => {
  test('shows a beginner-friendly empty timeline state', () => {
    render(<TimelineView />)

    expect(screen.getByTestId('timeline-view')).toBeDefined()
    expect(screen.getByText('Start a task to build the timeline')).toBeDefined()
    expect(
      screen.getByText(
        'Start a task or open a running task. Status changes will appear here in time order.'
      )
    ).toBeDefined()
    expect(screen.getByText('Start a task from the board')).toBeDefined()
    expect(
      screen.getByText('Watch tasks move through waiting, working, help needed, and finished steps')
    ).toBeDefined()
    expect(
      screen.getByText('Open a task when the timeline shows something that needs attention')
    ).toBeDefined()
    expect(screen.queryByText('No timeline events yet')).toBeNull()
    expect(screen.queryByText(/blocked and completed/i)).toBeNull()
  })

  test('keeps the timeline canvas mounted for the route smoke test', () => {
    render(<TimelineView />)

    expect(document.querySelector('canvas.timeline-canvas')).toBeTruthy()
    expect(canvasContext.fillText).toHaveBeenCalledWith('Waiting for run events', 320, 136)
  })
})
