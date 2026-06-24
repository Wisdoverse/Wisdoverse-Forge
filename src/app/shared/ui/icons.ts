function svg(
  size: number,
  content: string,
  opts?: { fill?: boolean; strokeWidth?: string }
): string {
  const strokeWidth = opts?.strokeWidth ?? '2'
  const fill = opts?.fill ? 'currentColor' : 'none'
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="${fill}" stroke="currentColor" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round">${content}</svg>`
}

export const iconSuccess = svg(14, '<polyline points="20 6 9 17 4 12"/>')
