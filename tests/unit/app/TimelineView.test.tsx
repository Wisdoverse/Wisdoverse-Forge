import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { TimelineView } from '@app/widgets/views/TimelineView'
import { useBoardStore } from '@app/shared/model/board.store'

class ResizeObserverStub {
  observe = vi.fn()
  disconnect = vi.fn()
}

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', ResizeObserverStub)
  vi.spyOn(HTMLCanvasElement.prototype, 'getBoundingClientRect').mockReturnValue({
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: 640,
    bottom: 360,
    width: 640,
    height: 360,
    toJSON: () => ({}),
  } as DOMRect)
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    setTransform: vi.fn(),
    clearRect: vi.fn(),
    fillRect: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    stroke: vi.fn(),
    fillText: vi.fn(),
  } as unknown as CanvasRenderingContext2D)
  useBoardStore.getState().reset()
  useBoardStore.getState().setViewMode('timeline')
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  useBoardStore.getState().reset()
})

describe('TimelineView', () => {
  test('guides users back to the task board before timeline updates exist', () => {
    render(<TimelineView />)

    const timeline = screen.getByTestId('timeline-view')
    expect(timeline).toHaveAccessibleName('Timeline view')
    expect(within(timeline).getByText('Open the task board to start the timeline')).toBeDefined()
    expect(
      within(timeline).getByText(
        'Create a task or open one that is already running. Timeline updates appear here after work starts.'
      )
    ).toBeDefined()
    expect(within(timeline).getByText('Choose Open task board')).toBeDefined()
    expect(
      within(timeline).getByText('Create a small task or open one that is already running')
    ).toBeDefined()
    expect(
      within(timeline).getByText(
        'Return to Timeline to see waiting, working, help needed, and finished updates'
      )
    ).toBeDefined()
    expect(timeline.textContent).not.toMatch(/refresh|event stream|queue/i)

    fireEvent.click(within(timeline).getByRole('button', { name: /open task board/i }))

    expect(useBoardStore.getState().viewMode).toBe('board')
  })
})
