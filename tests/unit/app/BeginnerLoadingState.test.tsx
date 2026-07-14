import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'

afterEach(cleanup)

describe('BeginnerLoadingState', () => {
  test('uses a quiet bordered loading frame for beginner guidance', () => {
    render(
      <BeginnerLoadingState
        title="Checking projects"
        detail="We are checking whether projects are available here."
        nextStep="If this takes more than a moment, open Projects again or ask an owner or admin to help with Projects."
      />
    )

    const status = screen.getByRole('status', { name: 'Checking projects' })
    expect(status).toHaveTextContent('Checking projects')
    expect(status).toHaveTextContent('open Projects again')
    expect(status.querySelectorAll('p')).toHaveLength(3)
    expect(status.className).toContain('rounded-md')
    expect(status.className).toContain('border')
    expect(status.className).not.toContain('rounded-lg')
    expect(status.className).not.toContain('border-dashed')
    const spinner = status.querySelector('svg')
    expect(spinner?.getAttribute('class') ?? '').toContain('text-secondary-light')
    expect(spinner?.getAttribute('class') ?? '').not.toContain('text-apple-blue')
  })
})
