import { useEffect, useRef } from 'react'

const TIMELINE_STEPS = [
  'Start a task from the board',
  'Watch queued, working, blocked, and completed updates',
  'Open a task when the timeline shows something that needs attention',
]

function drawTimeline(canvas: HTMLCanvasElement): void {
  let ctx: CanvasRenderingContext2D | null = null
  try {
    ctx = canvas.getContext('2d')
  } catch {
    return
  }
  if (!ctx) return

  const dpr = window.devicePixelRatio || 1
  const rect = canvas.getBoundingClientRect()
  const width = Math.max(1, Math.floor(rect.width * dpr))
  const height = Math.max(1, Math.floor(rect.height * dpr))

  canvas.width = width
  canvas.height = height
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, rect.width, rect.height)

  ctx.fillStyle = '#0b1020'
  ctx.fillRect(0, 0, rect.width, rect.height)

  const midY = rect.height / 2
  const startX = 40
  const endX = Math.max(startX, rect.width - 40)

  ctx.strokeStyle = 'rgba(148, 163, 184, 0.35)'
  ctx.lineWidth = 1
  ctx.beginPath()
  ctx.moveTo(startX, midY)
  ctx.lineTo(endX, midY)
  ctx.stroke()

  for (let i = 0; i <= 4; i += 1) {
    const x = startX + ((endX - startX) * i) / 4
    ctx.beginPath()
    ctx.moveTo(x, midY - 8)
    ctx.lineTo(x, midY + 8)
    ctx.stroke()
  }

  ctx.fillStyle = 'rgba(226, 232, 240, 0.78)'
  ctx.font = '13px Inter, ui-sans-serif, system-ui, sans-serif'
  ctx.textAlign = 'center'
  ctx.fillText('Waiting for run events', rect.width / 2, midY - 24)
}

export function TimelineView() {
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const render = () => drawTimeline(canvas)
    render()

    const observer = new ResizeObserver(render)
    observer.observe(canvas)
    window.addEventListener('resize', render)

    return () => {
      observer.disconnect()
      window.removeEventListener('resize', render)
    }
  }, [])

  return (
    <div
      data-testid="timeline-view"
      className="relative h-full w-full overflow-hidden bg-[#0b1020]"
      aria-label="Timeline view"
    >
      <canvas
        ref={canvasRef}
        className="timeline-canvas h-full w-full"
        aria-label="Timeline background"
      />
      <section
        aria-label="Timeline status"
        className="pointer-events-none absolute inset-0 flex items-center justify-center p-4"
      >
        <div className="max-w-lg rounded-lg border border-white/10 bg-black/35 px-4 py-3 text-white shadow-lg backdrop-blur">
          <p className="text-ui-body font-semibold">No timeline events yet</p>
          <p className="mt-1 text-ui-caption leading-relaxed text-white/68">
            Start a task or open a running task. Status changes will appear here in time order.
          </p>
          <ol className="mt-3 grid gap-1.5 text-ui-caption text-white/72">
            {TIMELINE_STEPS.map((step, index) => (
              <li key={step} className="flex gap-2">
                <span className="shrink-0 tabular-nums text-white/45">{index + 1}.</span>
                <span>{step}</span>
              </li>
            ))}
          </ol>
        </div>
      </section>
    </div>
  )
}
