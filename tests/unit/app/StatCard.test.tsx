import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import { StatCard } from '@app/features/analytics/StatCard'

afterEach(cleanup)

describe('StatCard', () => {
  test('shows a readable loading state for beginner operators', () => {
    render(<StatCard title="Total Events" value={0} loading />)

    expect(screen.getByText('Total Events')).toBeInTheDocument()
    expect(screen.getByText('Loading')).toBeInTheDocument()
    expect(screen.getByText('Loading').closest('[aria-busy="true"]')).toBeInTheDocument()
  })

  test('shows the metric value once loading finishes', () => {
    render(<StatCard title="Online" value={3} subtitle="Agents ready for work" accent="blue" />)

    expect(screen.getByText('Online')).toBeInTheDocument()
    expect(screen.getByText('3')).toBeInTheDocument()
    expect(screen.getByText('Agents ready for work')).toBeInTheDocument()
    expect(screen.queryByText('Loading')).not.toBeInTheDocument()
  })
})
