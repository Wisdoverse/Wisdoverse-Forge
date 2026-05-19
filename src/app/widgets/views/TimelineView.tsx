import { useEffect, useRef } from 'react'

function drawTimeline(canvas: HTMLCanvasElement): void {
  const ctx = canvas.getContext('2d')
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
  ctx.fillText(
    'Timeline view is ready; live orchestration events appear here.',
    rect.width / 2,
    midY - 24
  )
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
    <canvas ref={canvasRef} className="timeline-canvas h-full w-full" aria-label="Timeline view" />
  )
}
