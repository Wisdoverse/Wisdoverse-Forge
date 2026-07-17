import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'
import { MarkdownDoc } from '@app/shared/ui/markdown/MarkdownDoc'

describe('MarkdownDoc', () => {
  test('renders headings, lists, and inline code with house classes', () => {
    render(
      <MarkdownDoc text={'# Title\n\n## Phase A\n\n- first `chip` item\n- second item'} />
    )
    // h1 inside content demotes to the section scale (page already owns the real H1)
    const h1 = screen.getByRole('heading', { level: 2, name: 'Title' })
    expect(h1.className).toContain('text-ui-title')
    expect(screen.getByRole('heading', { level: 3, name: 'Phase A' })).toBeDefined()
    expect(screen.getAllByRole('listitem')).toHaveLength(2)
    expect(screen.getByText('chip').className).toContain('font-mono')
  })

  test('external links open safely', () => {
    render(<MarkdownDoc text={'see [docs](https://example.com/x)'} />)
    const link = screen.getByRole('link', { name: 'docs' })
    expect(link.getAttribute('rel')).toBe('noopener noreferrer')
    expect(link.getAttribute('target')).toBe('_blank')
  })

  test('keeps raw HTML escaped (no rehype-raw)', () => {
    render(<MarkdownDoc text={'<img src=x onerror=alert(1)>hello'} />)
    expect(document.querySelector('img')).toBeNull()
    expect(screen.getByText(/hello/)).toBeDefined()
  })

  test('renders GFM tables and fenced code blocks', () => {
    render(<MarkdownDoc text={'| a | b |\n| - | - |\n| 1 | 2 |\n\n```\nconst x = 1\n```'} />)
    expect(screen.getByRole('table')).toBeDefined()
    expect(screen.getByText('const x = 1').closest('pre')).not.toBeNull()
  })

  test('renders mixed zh/EN prose as paragraphs', () => {
    render(<MarkdownDoc text={'此前，用户阅读 a long line of 中英混排 text。\n\nSecond paragraph.'} />)
    expect(screen.getByText(/中英混排/)).toBeDefined()
    expect(screen.getByText('Second paragraph.')).toBeDefined()
  })
})
